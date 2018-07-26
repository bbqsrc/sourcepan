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
        let branch = &self.repo.find_branch(&self.branch, git2::BranchType::Local).expect("find branch");
        let refr = branch.get().name().expect("find branch name as ref");
        let mut revwalk = (&self.repo).revwalk().expect("get a revwalk");
        revwalk.push_ref(refr).expect("push ref successfully");

        let infos: Vec<CommitInfo> = revwalk
            .filter(|x| x.is_ok())
            .map(|x| (&self.repo).find_commit(x.expect("item to unwrap")).expect("commit to be found"))
            .map(|commit| {
                let author = commit.author();
                let subid = &format!("{}", commit.id())[0..7];
                let full_summary = commit.summary().expect("valid summary");
                let summary = &format!("{}", &full_summary)[0..min(80, full_summary.len())];

                let naive_dt = chrono::Utc.timestamp(commit.time().seconds(), 0).naive_utc();
                let offset = chrono::offset::FixedOffset::east(commit.time().offset_minutes() * 60);
                let date: chrono::DateTime<chrono::FixedOffset> = chrono::DateTime::from_utc(naive_dt, offset);

                CommitInfo {
                    summary: summary.to_string(),
                    short_id: subid.to_string(),
                    author: format!("{} <{}>", author.name().expect("valid author name"), author.email().expect("valid author email")),
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
    file_status_view: Rc<FileStatusView>,
    unstaged_view: Rc<FileStatusView>,
    root: gtk::Paned
}

trait FileStatusViewable {
    fn new() -> Rc<Self>;
    fn update_list(&self, statuses: &[&git2::StatusEntry], is_editable: bool);
}

struct FileStatusPresenter<V> {
    view: RefCell<Weak<V>>
}

impl<V: FileStatusViewable> FileStatusPresenter<V> {
    fn new() -> FileStatusPresenter<V> {
        FileStatusPresenter {
            view: RefCell::new(Weak::new())
        }
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("Presenter only running while view still exists")
    }

    fn set_statuses(&self, statuses: &[&git2::StatusEntry]) {
        self.view().update_list(statuses, false);
    }
}

struct FileStatusView {
    presenter: FileStatusPresenter<FileStatusView>,
    status_list_store: gtk::ListStore,
    root: gtk::ScrolledWindow
}

impl FileStatusViewable for FileStatusView {
    fn new() -> Rc<FileStatusView> {
        let (list_store, root) = FileStatusView::create();

        let view = Rc::new(FileStatusView {
            presenter: FileStatusPresenter::new(),
            status_list_store: list_store,
            root: root
        });

        *view.presenter.view.borrow_mut() = Rc::downgrade(&view);

        view
    }

    fn update_list(&self, statuses: &[&git2::StatusEntry], is_editable: bool) {
        self.status_list_store.clear();

        for entry in statuses.iter() {
            self.status_list_store.insert_with_values(None, &[0, 1], &[
                &format!("{:?}", entry.status()),
                &entry.path().unwrap()
            ]);
        }
    }
}

impl FileStatusView {
    fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    fn create() -> (gtk::ListStore, gtk::ScrolledWindow) {
        let list_store = gtk::ListStore::new(&[
            String::static_type(),
            String::static_type()
        ]);

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
        treeview.set_headers_visible(false);

        append_column(&treeview, 0, "Status");
        append_column(&treeview, 1, "Path");

        treeview.set_model(&list_store);

        let scroller = gtk::ScrolledWindow::new(None, None);
        scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        scroller.add(&treeview);

        (list_store, scroller)
    }
}

trait GitStatusExt {
    fn is_in_index(&self) -> bool;
}

impl GitStatusExt for git2::Status {
    fn is_in_index(&self) -> bool {
        self.is_index_new() ||
            self.is_index_modified() ||
            self.is_index_deleted() ||
            self.is_index_renamed() ||
            self.is_index_typechange()
    }
}

impl BranchViewable for BranchView {
    fn new(repo: Rc<git2::Repository>, branch: String) -> Rc<BranchView> {
        let list_store = gtk::ListStore::new(&[
            String::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type()
        ]);

        let (file_status_view, unstaged_view, root) = BranchView::create(&list_store);

        let view = Rc::new(BranchView {
            presenter: BranchPresenter::new(repo, branch),
            history_list_store: list_store,
            file_status_view: file_status_view,
            unstaged_view: unstaged_view,
            root: root
        });

        *view.presenter.view.borrow_mut() = Rc::downgrade(&view);

        // Init history
        // view.presenter.load_status();
        let commits = view.presenter.load_commit_history();
        view.history_list_store.clear();

        // Add setinel for uncommitted content
        view.history_list_store.insert_with_values(None, &[0, 1, 2, 3], &[
            &"Uncommitted changes",
            &"*",
            &"*",
            &"*"
        ]);

        // Init file view
        {
            let statuses = view.presenter.repo.statuses(None).unwrap();
            let entries: Vec<git2::StatusEntry> = statuses.iter().collect();

            let mut unstaged = vec![];
            let mut staged = vec![];

            for entry in entries.iter() {
                if entry.status().is_ignored() {
                    continue
                }

                if !entry.status().is_in_index() {
                    unstaged.push(entry);
                } else {
                    staged.push(entry);
                }
            }

            view.unstaged_view.presenter.set_statuses(&unstaged);
            view.file_status_view.presenter.set_statuses(&staged);
        }

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

    fn create(model: &gtk::ListStore) -> (Rc<FileStatusView>, Rc<FileStatusView>, gtk::Paned) {
        let treeview = BranchView::create_tree(model);

        // Make tree view scrollable
        let commit_history = gtk::ScrolledWindow::new(None, None);
        commit_history.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        commit_history.add(&treeview);

        let selected_files = FileStatusView::new();
        let unstaged_files = FileStatusView::new();
        let diff_view = gtk::Label::new("Diff view TODO");

        let main_pane = gtk::Paned::new(gtk::Orientation::Vertical);
        let file_pane = gtk::Paned::new(gtk::Orientation::Vertical);
        let bottom_pane = gtk::Paned::new(gtk::Orientation::Horizontal);

        // Add everything to the panes
        file_pane.pack1(selected_files.widget(), true, true);
        file_pane.pack2(unstaged_files.widget(), true, true);

        bottom_pane.pack1(&file_pane, true, true);
        bottom_pane.pack2(&diff_view, true, true);

        main_pane.pack1(&commit_history, true, true);
        main_pane.pack2(&bottom_pane, true, true);

        (selected_files, unstaged_files, main_pane)
    }
}
