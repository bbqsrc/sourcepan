use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::cmp::min;
use std::sync::mpsc::{channel, TryRecvError};
use std::time::Duration;

use notify::{DebouncedEvent, RecommendedWatcher, Watcher, RecursiveMode};
use chrono::{self, TimeZone};
use git2;
use gtk::prelude::*;
use gtk;
use gdk;

#[derive(Debug)]
pub struct CommitInfo {
    pub id: git2::Oid,
    pub summary: String,
    pub short_id: String,
    pub author: String,
    pub commit_date: String
}

impl CommitInfo {
    fn uncommitted_sentinel() -> CommitInfo {
        CommitInfo {
            id: git2::Oid::zero(),
            summary: "Uncommitted changes".into(),
            short_id: "*".into(),
            author: "*".into(),
            commit_date: "*".into()
        }
    }

    fn is_sentinel(&self) -> bool {
        self.id == git2::Oid::zero() && self.summary == "Uncommitted changes"
    }
}

struct BranchPresenter<V> {
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

    fn on_uncommitted_changes_selected(&self) {
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

    fn on_commit_selected<'a>(&self, info: &CommitInfo) {
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

        let view = Rc::new(FileStatusView {
            presenter: FileStatusPresenter::new(),
            status_list_store: list_store,
            root: root
        });

        *view.presenter.view.borrow_mut() = Rc::downgrade(&view);

        view
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

        treeview.connect_button_press_event(|_, event| {
            use std::ops::Deref;

            if event.get_button() == 3 { // Right click
                let menu = gtk::Menu::new();
                menu.append(&gtk::MenuItem::new_with_label("Test 1"));
                menu.append(&gtk::MenuItem::new_with_label("Test 2"));
                menu.append(&gtk::MenuItem::new_with_label("Test 3"));
                menu.popup_at_pointer(Some(event.deref()));
            }

            gtk::Inhibit(false)
        });

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

trait HistoryViewable {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<Self>;
    fn set_history(&self, commits: &[CommitInfo]);
    fn selected_row(&self) -> Option<usize>;
}

struct HistoryPresenter<V> {
    parent: Weak<BranchPresenter<BranchView>>,
    view: RefCell<Weak<V>>,
    commits: RefCell<Vec<CommitInfo>>,
    watcher: RefCell<RecommendedWatcher>
}

impl<V: HistoryViewable> HistoryPresenter<V> where V: 'static {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<HistoryPresenter<V>> {
        let (tx, rx) = channel();

        let presenter = Rc::new(HistoryPresenter {
            parent: parent,
            view: RefCell::new(Weak::new()),
            commits: RefCell::new(vec![]),
            watcher: RefCell::new(Watcher::new(tx, Duration::from_secs(2)).unwrap())
        });

        let weak_presenter = Rc::downgrade(&presenter);

        gtk::idle_add(move || {
            match rx.try_recv() {
                Err(err) => {
                    match err {
                        TryRecvError::Empty => gtk::Continue(true),
                        TryRecvError::Disconnected => {
                            gtk::Continue(false)
                        }
                    }
                },
                Ok(v) => {
                    match weak_presenter.upgrade() {
                        Some(p) => {
                            p.on_path_change_event(v);
                            gtk::Continue(true)
                        },
                        None => {
                            gtk::Continue(false)
                        }
                    }
                }
            }
        });

        presenter
    }

    fn on_path_change_event(&self, event: DebouncedEvent) {
        let maybe_path = match event {
            DebouncedEvent::NoticeWrite(path) => Some(path),
            DebouncedEvent::NoticeRemove(path) => Some(path),
            DebouncedEvent::Create(path) => Some(path),
            DebouncedEvent::Write(path) => Some(path),
            DebouncedEvent::Chmod(path) => Some(path),
            DebouncedEvent::Remove(path) => Some(path),
            DebouncedEvent::Rename(_, new_path) => Some(new_path),
            DebouncedEvent::Rescan => None,
            DebouncedEvent::Error(_, maybe_path) => maybe_path
        };

        let path = match maybe_path {
            Some(v) => v,
            None => return
        };

        let parent = self.parent();
        if path.ends_with("index") || path.ends_with(&parent.branch) || !path.components().any(|x| x.as_os_str() == ".git") {
            self.load_commit_history();
            if let Some(idx) = self.view().selected_row() {
                self.on_item_selected(idx);
            }
        }
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("Presenter only running while view still exists")
    }

    fn parent(&self) -> Rc<BranchPresenter<BranchView>> {
        self.parent
            .upgrade()
            .expect("Presenter only running while parent still exists")
    }

    fn has_uncommitted_changes(&self) -> bool {
        let parent = self.parent();
        let statuses = parent.repo.statuses(None).unwrap();
        statuses.iter().filter(|x| !x.status().is_ignored()).count() > 0
    }

    pub fn load_commit_history(&self) {
        let parent = self.parent();
        let branch = &parent.repo.find_branch(&self.parent().branch, git2::BranchType::Local).expect("find branch");
        let refr = branch.get().name().expect("find branch name as ref");
        let mut revwalk = (&parent.repo).revwalk().expect("get a revwalk");
        revwalk.push_ref(refr).expect("push ref successfully");

        let mut infos = vec![];

        if self.has_uncommitted_changes() {
            infos.push(CommitInfo::uncommitted_sentinel());
        }

        for rev in revwalk {
            let rev = match rev {
                Ok(v) => v,
                Err(_) => continue
            };

            let commit = (&parent.repo).find_commit(rev).expect("commit to be found");

            let author = commit.author();
            let subid = &format!("{}", commit.id())[0..7];
            let full_summary = commit.summary().expect("valid summary");
            let summary = &format!("{}", &full_summary)[0..min(80, full_summary.len())];

            let naive_dt = chrono::Utc.timestamp(commit.time().seconds(), 0).naive_utc();
            let offset = chrono::offset::FixedOffset::east(commit.time().offset_minutes() * 60);
            let date: chrono::DateTime<chrono::FixedOffset> = chrono::DateTime::from_utc(naive_dt, offset);

            let author_name = author.name().expect("valid author name");
            let author_email = author.email().expect("valid author email");

            let info = CommitInfo {
                id: commit.id(),
                summary: summary.to_string(),
                short_id: subid.to_string(),
                author: format!("{} <{}>", &author_name, &author_email),
                commit_date: date.to_string()
            };

            infos.push(info);
        }

        *self.commits.borrow_mut() = infos;

        self.view().set_history(&self.commits.borrow());
    }

    fn on_item_selected(&self, index: usize) {
        let info = &self.commits.borrow()[index];

        if info.is_sentinel() {
            self.parent().on_uncommitted_changes_selected();
        } else {
            self.parent().on_commit_selected(&info)
        }
    }

    fn start(&self) {
        self.load_commit_history();
        let parent = self.parent();

        self.watcher.borrow_mut()
            .watch(parent.repo.path().parent().unwrap(), RecursiveMode::Recursive)
            .unwrap();
    }
}

struct HistoryView {
    presenter: Rc<HistoryPresenter<HistoryView>>,
    list_store: gtk::ListStore,
    tree: gtk::TreeView,
    root: gtk::ScrolledWindow
}

impl HistoryView {
    fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

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
}

impl HistoryViewable for HistoryView {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<HistoryView> {
        let list_store = gtk::ListStore::new(&[
            String::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type()
        ]);

        let treeview = HistoryView::create_tree(&list_store);

        // Make tree view scrollable
        let root = gtk::ScrolledWindow::new(None, None);
        root.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        root.add(&treeview);

        let view = Rc::new(HistoryView {
            presenter: HistoryPresenter::new(parent),
            list_store: list_store,
            tree: treeview,
            root: root
        });

        *view.presenter.view.borrow_mut() = Rc::downgrade(&view);

        let weak_view = Rc::downgrade(&view);
        view.tree.connect_cursor_changed(move |_| {
            if let Some(view) = weak_view.upgrade() {
                if let Some(idx) = view.selected_row() {
                    view.presenter.on_item_selected(idx);
                }
            }
        });

        view.presenter.start();
        
        view
    }

    fn selected_row(&self) -> Option<usize> {
        if let Some(path) = self.tree.get_cursor().0 {
            if let Some(idx) = path.get_indices().first() {
                return Some(*idx as usize);
            }
        }

        None
    }

    fn set_history(&self, commits: &[CommitInfo]) {
        let cursor = self.tree.get_cursor();

        self.list_store.clear();

        for commit in commits {
            self.list_store.insert_with_values(None, &[0, 1, 2, 3], &[
                &commit.summary,
                &commit.short_id,
                &commit.author,
                &commit.commit_date
            ]);
        }

        let col = match cursor.1 {
            Some(ref v) => Some(v),
            None => None
        };

        if let Some(path) = cursor.0 {
            self.tree.set_cursor(&path, col, false);
        }
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

        let view = Rc::new(BranchView {
            presenter: Rc::clone(&presenter),
            history_view: history_view,
            file_status_view: file_status_view,
            unstaged_view: unstaged_view,
            root: root
        });

        *view.presenter.view.borrow_mut() = Rc::downgrade(&view);

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
