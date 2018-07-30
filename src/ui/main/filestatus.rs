use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::path::Path;

use git2;
use gtk::prelude::*;
use gtk;

use ui::main::TreeItem;
use super::branch::{BranchPresenter, BranchView};

pub trait FileStatusViewable {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<Self>;
    fn staged_view(&self) -> &FileListView;
    fn unstaged_view(&self) -> &FileListView;
    fn set_items(&self, staged: &[TreeItem], unstaged: &[TreeItem]);
    fn show_commit(&self, statuses: &[TreeItem], commit: &git2::Commit);
}

pub struct FileStatusPresenter<V> {
    parent: Weak<BranchPresenter<BranchView>>,
    view: RefCell<Weak<V>>
}

impl<V: FileStatusViewable> FileStatusPresenter<V> {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> FileStatusPresenter<V> {
        FileStatusPresenter {
            parent,
            view: RefCell::new(Weak::new())
        }
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("Presenter only running while view still exists")
    }

    pub fn set_history_statuses(&self, staged: &[TreeItem], unstaged: &[TreeItem]) {
        self.view().set_items(&staged, &unstaged);
    }

    pub fn set_overview_statuses(&self, statuses: &[TreeItem], commit: &git2::Commit) {
        self.view().show_commit(&statuses, commit);
    }

    fn parent(&self) -> Rc<BranchPresenter<BranchView>> {
        self.parent
            .upgrade()
            .expect("Parent presenter to work")
    }

    fn on_toggle_staged(&self, index: usize) {
        let parent = self.parent();
        let repo = parent.repo();

        {
            let delta = &parent.deltas().borrow().0[index];
            let head = repo.head().unwrap().peel(git2::ObjectType::Commit).unwrap();
            repo.reset_default(Some(&head), Path::new(&delta.path)).unwrap();
        }

        repo.index().unwrap().write().unwrap();
        parent.on_uncommitted_changes_selected();
    }

    fn on_toggle_unstaged(&self, index: usize) {
        let parent = self.parent();
        let repo = parent.repo();

        {
            let delta = &parent.deltas().borrow().1[index];
            repo.index().unwrap().add_path(&Path::new(&delta.path)).unwrap();
        }

        repo.index().unwrap().write().unwrap();
        parent.on_uncommitted_changes_selected();
    }
}

pub struct OverviewView {
    label: gtk::Label,
    root: gtk::ScrolledWindow
}

impl OverviewView {
    fn new() -> OverviewView {
        let label = gtk::Label::new("");
        label.set_selectable(true);

        let root = gtk::ScrolledWindow::new(None, None);
        root.get_style_context().unwrap().add_class("white-background");
        
        root.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        root.add(&label);

        OverviewView {
            label,
            root
        }
    }

    fn set_commit(&self, commit: &git2::Commit) {
        use super::history::HumanCommitExt;

        let mut msg = String::new();

        msg.push_str("Commit: ");
        msg.push_str(&commit.id_str());
        msg.push('\n');

        msg.push_str("Author: ");
        msg.push_str(&commit.author_str());
        msg.push('\n');

        msg.push_str("Date: ");
        msg.push_str(&commit.date().to_string());
        msg.push('\n');
        msg.push('\n');

        self.label.set_label(&msg);
    }

    fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }
}

pub struct FileStatusView {
    pub presenter: FileStatusPresenter<FileStatusView>,
    staged_view: FileListView,
    unstaged_view: FileListView,
    overview_view: OverviewView,
    root: gtk::Paned
}

impl FileStatusViewable for FileStatusView {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<FileStatusView> {
        let root = gtk::Paned::new(gtk::Orientation::Vertical);

        let staged_view = FileListView::new();
        let unstaged_view = FileListView::new();
        let overview_view = OverviewView::new();

        let view = view!(FileStatusView {
            presenter: FileStatusPresenter::new(parent),
            staged_view,
            unstaged_view,
            overview_view,
            root
        });

        view.staged_view.selection_cell.connect_toggled(weak!(view => move |_, tree_path| {
            if let Some(view) = view.upgrade() {
                if let Some(index) = tree_path.get_indices().first() {
                    if index < &0 {
                        return;
                    }

                    view.presenter.on_toggle_staged(*index as usize);
                }
            }
        }));

        view.unstaged_view.selection_cell.connect_toggled(weak!(view => move |_, tree_path| {
            if let Some(view) = view.upgrade() {
                if let Some(index) = tree_path.get_indices().first() {
                    if index < &0 {
                        return;
                    }

                    view.presenter.on_toggle_unstaged(*index as usize);
                }
            }
        }));

        view
    }

    fn staged_view(&self) -> &FileListView {
        &self.staged_view
    }

    fn unstaged_view(&self) -> &FileListView {
        &self.unstaged_view
    }

    fn set_items(&self, staged: &[TreeItem], unstaged: &[TreeItem]) {
        self.staged_view().set_items(&staged);
        self.unstaged_view().set_items(&unstaged);

        let staged = self.staged_view.root.clone();
        let unstaged = self.unstaged_view.root.clone();

        for child in self.root.get_children() {
            self.root.remove(&child);
        }

        self.staged_view.columns[0].set_visible(true);

        self.root.add1(&staged);
        self.root.add2(&unstaged);

        self.root.show_all();
    }

    fn show_commit(&self, statuses: &[TreeItem], commit: &git2::Commit) {
        self.staged_view().set_items(&statuses);
        self.unstaged_view().set_items(&[]);

        for child in self.root.get_children() {
            self.root.remove(&child);
        }

        self.overview_view.set_commit(commit);
        self.staged_view.columns[0].set_visible(false);

        self.root.add1(self.staged_view.widget());
        self.root.add2(self.overview_view.widget());
        self.root.show_all();
    }
}

pub struct FileListView {
    list_store: gtk::ListStore,
    columns: [gtk::TreeViewColumn; 3],
    selection_cell: gtk::CellRendererToggle,
    root: gtk::ScrolledWindow
}

impl FileListView {
    fn new() -> FileListView {
        let list_store = gtk::ListStore::new(&[
            bool::static_type(),
            String::static_type(),
            String::static_type()
        ]);

        fn append_column(tree: &gtk::TreeView, id: i32, title: &str) -> gtk::TreeViewColumn {
            let column = gtk::TreeViewColumn::new();
            let cell = gtk::CellRendererText::new();

            column.pack_start(&cell, true);
            column.set_resizable(true);
            column.set_title(title);
            column.add_attribute(&cell, "text", id);
            tree.append_column(&column);

            column
        }

        let treeview = gtk::TreeView::new();
        treeview.set_headers_visible(false);

        let column = gtk::TreeViewColumn::new();
        let cell = gtk::CellRendererToggle::new();
        cell.set_activatable(true);
        column.pack_start(&cell, true);
        column.set_resizable(true);
        column.set_title("Selected");
        column.add_attribute(&cell, "active", 0);
        treeview.append_column(&column);

        let status_col = append_column(&treeview, 1, "Status");
        let path_col = append_column(&treeview, 2, "Path");

        treeview.set_model(&list_store);

        let scroller = gtk::ScrolledWindow::new(None, None);
        scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        scroller.add(&treeview);

        FileListView {
            list_store: list_store,
            columns: [column, status_col, path_col],
            selection_cell: cell,
            root: scroller
        }
    }

    fn set_items(&self, statuses: &[TreeItem]) {
       self.list_store.clear();

        for entry in statuses.iter() {
            self.list_store.insert_with_values(None, &[0, 1, 2], &[
                &entry.is_selected,
                &format!("{:?}", entry.delta),
                &entry.path
            ]);
        }
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }
}

impl FileStatusView {
    pub fn widget(&self) -> &gtk::Paned {
        &self.root
    }
}
