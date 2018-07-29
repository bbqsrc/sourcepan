use std::rc::{Rc, Weak};
use std::cell::RefCell;

use git2;
use gtk::prelude::*;
use gtk;

use super::history::{HistoryView, HistoryViewable};
use super::CommitInfo;

pub struct BranchPresenter<V> {
    view: RefCell<Weak<V>>,
    repo: Rc<git2::Repository>,
    branch: String
}

#[derive(Debug)]
pub struct TreeItem {
    id: git2::Oid,
    path: String,
    delta: git2::Delta
}

impl<V: BranchViewable> BranchPresenter<V> {
    fn new(repo: Rc<git2::Repository>, branch: String) -> BranchPresenter<V> {
        BranchPresenter {
            view: RefCell::new(Weak::new()),
            repo: repo,
            branch: branch
        }
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn repo(&self) -> &git2::Repository {
        &self.repo
    }

    pub fn on_uncommitted_changes_selected(&self) {
        let repo_head_tree = self.repo.head().unwrap().peel_to_tree().unwrap();
        let mut diff_opts = git2::DiffOptions::new();
        diff_opts
            .include_untracked(true)
            .recurse_untracked_dirs(true);

        let mut workdir_diff = self.repo.diff_tree_to_workdir_with_index(Some(&repo_head_tree), Some(&mut diff_opts)).unwrap();
        workdir_diff.find_similar(None).unwrap();

        let mut index_diff = self.repo.diff_tree_to_index(Some(&repo_head_tree), None, None).unwrap();
        index_diff.find_similar(None).unwrap();

        let index_deltas: Vec<TreeItem> = index_diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status()
            }
        }).collect();

        let workdir_deltas: Vec<TreeItem> = workdir_diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status()
            }
        })
        .filter(|x| !index_deltas.iter().any(|y| x.id == y.id))
        .collect();

        self.view().set_staged_statuses(&index_deltas);
        self.view().set_unstaged_statuses(&workdir_deltas);
    }

    pub fn on_commit_selected<'a>(&self, info: &CommitInfo) {
        let commit = self.repo.find_commit(info.id).expect("Commit to exist in repo");
        let parent_commits: Vec<git2::Commit> = commit.parents().collect();

        let maybe_parent = parent_commits.first().map(|x| x.tree().unwrap());
        let parent = match maybe_parent {
            Some(ref v) => Some(v),
            None => None
        };

        let mut diff = self.repo.diff_tree_to_tree(
            parent,
            Some(&commit.tree().unwrap()),
            None
        ).expect("a diff");

        diff.find_similar(None).unwrap();

        let deltas: Vec<TreeItem> = diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status()
            }
        }).collect();

        self.view().set_staged_statuses(&deltas);
        self.view().set_unstaged_statuses(&[]);
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("BranchPresenter only running while view still exists")
    }
}

pub trait BranchViewable {
    fn new(repo: Rc<git2::Repository>, branch: String) -> Rc<Self>;
    fn widget(&self) -> &gtk::Paned;
    fn set_staged_statuses(&self, statuses: &[TreeItem]);
    fn set_unstaged_statuses(&self, statuses: &[TreeItem]);
}

#[allow(dead_code)]
pub struct BranchView {
    presenter: Rc<BranchPresenter<BranchView>>,
    history_view: Rc<HistoryView>,
    file_status_view: Rc<FileStatusView>,
    unstaged_view: Rc<FileStatusView>,
    root: gtk::Paned
}

trait FileStatusViewable {
    fn new() -> Rc<Self>;
    fn update_list(&self, statuses: &[TreeItem]);
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

    fn set_history_statuses(&self, statuses: &[TreeItem]) {
        self.view().update_list(statuses);
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

        view!(FileStatusView {
            presenter: FileStatusPresenter::new(),
            status_list_store: list_store,
            root: root
        })
    }

    fn update_list(&self, statuses: &[TreeItem]) {
        self.status_list_store.clear();

        for entry in statuses.iter() {
            self.status_list_store.insert_with_values(None, &[0, 1], &[
                &format!("{:?}", entry.delta),
                &entry.path
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
    fn set_staged_statuses(&self, statuses: &[TreeItem]) {
        self.file_status_view.presenter.set_history_statuses(statuses);
    }
    
    fn set_unstaged_statuses(&self, statuses: &[TreeItem]) {
        self.unstaged_view.presenter.set_history_statuses(statuses);
    }

    fn new(repo: Rc<git2::Repository>, branch: String) -> Rc<BranchView> {
        let presenter = Rc::new(BranchPresenter::new(repo, branch));

        let (history_view, file_status_view, unstaged_view, root) = BranchView::create(Rc::downgrade(&presenter));

        let view = view!(BranchView {
            presenter: Rc::clone(&presenter),
            history_view: history_view,
            file_status_view: file_status_view,
            unstaged_view: unstaged_view,
            root: root
        });

        view
    }

    fn widget(&self) -> &gtk::Paned {
        &self.root
    }
}

impl BranchView {
    fn create(parent: Weak<BranchPresenter<BranchView>>) -> (Rc<HistoryView>, Rc<FileStatusView>, Rc<FileStatusView>, gtk::Paned) {
        let commit_history = HistoryView::new(parent);
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

        main_pane.pack1(commit_history.widget(), true, true);
        main_pane.pack2(&bottom_pane, true, true);

        (commit_history, selected_files, unstaged_files, main_pane)
    }
}
