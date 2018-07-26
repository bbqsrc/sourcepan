use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::cmp::min;

use chrono::{self, TimeZone};
use git2;
use gtk::prelude::*;
use gtk;

pub struct CommitInfo {
    pub summary: String,
    pub short_id: String,
    pub author: String,
    pub commit_date: String
}

struct BranchPresenter<V> {
    view: RefCell<Weak<V>>,
    repo: Rc<git2::Repository>,
    branch: String
}

impl<V: BranchViewable> BranchPresenter<V> {
    fn new(repo: Rc<git2::Repository>, branch: String) -> BranchPresenter<V> {
        BranchPresenter {
            view: RefCell::new(Weak::new()),
            repo: repo,
            branch: branch
        }
    }

    pub fn load_commit_history(&self) -> Vec<CommitInfo> {
        let branch = &self.repo.find_branch(&self.branch, git2::BranchType::Local).unwrap();
        let refr = branch.get().name().unwrap();
        let mut revwalk = (&self.repo).revwalk().unwrap();
        revwalk.push_ref(refr).unwrap();

        let infos: Vec<CommitInfo> = revwalk
            .map(|x| (&self.repo).find_commit(x.unwrap()).unwrap())
            .map(|commit| {
                let author = commit.author();
                let subid = &format!("{}", commit.id())[0..7];
                let full_summary = commit.summary().unwrap();
                let summary = &format!("{}", &full_summary)[0..min(80, full_summary.len())];

                let naive_dt = chrono::Utc.timestamp(commit.time().seconds(), 0).naive_utc();
                let offset = chrono::offset::FixedOffset::east(commit.time().offset_minutes() * 60);
                let date: chrono::DateTime<chrono::FixedOffset> = chrono::DateTime::from_utc(naive_dt, offset);

                CommitInfo {
                    summary: summary.to_string(),
                    short_id: subid.to_string(),
                    author: format!("{} <{}>", author.name().unwrap(), author.email().unwrap()),
                    commit_date: date.to_string()
                }
            })
            .collect();

        infos
    }
}

pub trait BranchViewable {
    fn new(repo: Rc<git2::Repository>, branch: String) -> Rc<Self>;
    fn widget(&self) -> &gtk::Paned;
}

pub struct BranchView {
    presenter: BranchPresenter<BranchView>,
    history_list_store: gtk::ListStore,
    root: gtk::Paned
}

impl BranchViewable for BranchView {
    fn new(repo: Rc<git2::Repository>, branch: String) -> Rc<BranchView> {
        let list_store = gtk::ListStore::new(&[
            String::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type()
        ]);

        let root = BranchView::create(&list_store);

        let view = Rc::new(BranchView {
            presenter: BranchPresenter::new(repo, branch),
            history_list_store: list_store,
            root: root
        });

        *view.presenter.view.borrow_mut() = Rc::downgrade(&view);

        // Init history
        let commits = view.presenter.load_commit_history();
        view.history_list_store.clear();
        for commit in commits {
            view.history_list_store.insert_with_values(None, &[0, 1, 2, 3], &[
                &commit.summary,
                &commit.short_id,
                &commit.author,
                &commit.commit_date
            ]);
        }

        view
    }

    fn widget(&self) -> &gtk::Paned {
        &self.root
    }
}

impl BranchView {
    fn create_tree(model: &gtk::ListStore) -> gtk::TreeView {
        fn append_column(tree: &gtk::TreeView, id: i32, title: &str) {
            let column = gtk::TreeViewColumn::new();
            let cell = gtk::CellRendererText::new();

            column.pack_start(&cell, true);
            column.set_resizable(true);
            column.set_title(title);
            column.add_attribute(&cell, "text", id);
            tree.append_column(&column);
        }

        let treeview = gtk::TreeView::new();

        append_column(&treeview, 0, "Summary");
        append_column(&treeview, 1, "Commit");
        append_column(&treeview, 2, "Author");
        append_column(&treeview, 3, "Date");

        treeview.set_model(model);
        treeview
    }

    fn create(model: &gtk::ListStore) -> gtk::Paned {
        let treeview = BranchView::create_tree(model);

        // Make tree view scrollable
        let commit_history = gtk::ScrolledWindow::new(None, None);
        commit_history.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        commit_history.add(&treeview);

        let selected_files = gtk::Label::new("Selected files TODO");
        let unstaged_files = gtk::Label::new("Unstaged TODO");
        let diff_view = gtk::Label::new("Diff view TODO");

        let main_pane = gtk::Paned::new(gtk::Orientation::Vertical);
        let file_pane = gtk::Paned::new(gtk::Orientation::Vertical);
        let bottom_pane = gtk::Paned::new(gtk::Orientation::Horizontal);

        // Add everything to the panes
        file_pane.pack1(&selected_files, true, true);
        file_pane.pack2(&unstaged_files, true, true);

        bottom_pane.pack1(&file_pane, true, true);
        bottom_pane.pack2(&diff_view, true, true);

        main_pane.pack1(&commit_history, true, true);
        main_pane.pack2(&bottom_pane, true, true);

        main_pane
    }
}
