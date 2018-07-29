use std::rc::{Rc, Weak};
use std::cell::RefCell;

use git2;
use gtk::prelude::*;
use gtk;

use ui::main::TreeItem;

pub trait FileStatusViewable {
    fn new() -> Rc<Self>;
    fn update_list(&self, statuses: &[TreeItem]);
}

pub struct FileStatusPresenter<V> {
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

    pub fn set_history_statuses(&self, statuses: &[TreeItem]) {
        self.view().update_list(statuses);
    }
}

pub struct FileStatusView {
    pub presenter: FileStatusPresenter<FileStatusView>,
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

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
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
