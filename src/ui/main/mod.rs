// Sourcepan - a Gtk+ Git client written in Rust
// Copyright (C) 2018  Brendan Molloy <brendan@bbqsrc.net>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3 as 
// published by the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

mod branch;
mod history;
mod filestatus;
mod diff;

use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::path::Path;
use std::fmt;

use git2;
use gtk::prelude::*;
use gtk;

use ui::Window;
use ui::main::branch::{BranchViewable, BranchView};
use ui::AsMessageDialog;

#[derive(Debug)]
pub struct CommitInfo {
    pub id: git2::Oid,
    summary: String,
    pub short_id: String,
    pub author: String,
    pub commit_date: String,
    pub branch_heads: Vec<String>
}

impl CommitInfo {
    pub fn summary(&self) -> String {
        if self.branch_heads.len() == 0 {
            return self.summary.to_string();
        }

        let mut out = String::new();
        
        for name in &self.branch_heads {
            out.push_str(&format!("[{}] ", &name));
        }
        out.push_str(&self.summary);
        out
    } 
}

const UNCOMMITTED_STR: &'static str = "<b>Uncommitted changes</b>";

impl CommitInfo {
    pub fn uncommitted_sentinel() -> CommitInfo {
        CommitInfo {
            id: git2::Oid::zero(),
            summary: UNCOMMITTED_STR.into(),
            short_id: "*".into(),
            author: "*".into(),
            commit_date: "*".into(),
            branch_heads: vec![]
        }
    }

    pub fn is_sentinel(&self) -> bool {
        self.id == git2::Oid::zero() && self.summary == UNCOMMITTED_STR
    }
}

struct MainPresenter<V> {
    view: RefCell<Weak<V>>,
    repo: RefCell<Rc<git2::Repository>>
}

pub trait MainViewable {
    fn with_repo(repo: git2::Repository) -> Rc<Self>;
    fn set_branches(&self, repo: Rc<git2::Repository>, branches: Vec<String>);
    fn set_branch_by_index(&self, index: usize);
    fn show(&self);
    fn set_title(&self, path: &str);
    fn open_repo_selector(&self);
    fn handle_error(&self, error: impl fmt::Display);
}

impl<V: MainViewable> MainPresenter<V> {
    fn new(repo: Rc<git2::Repository>) -> MainPresenter<V> {
        MainPresenter {
            view: RefCell::new(Weak::new()),
            repo: RefCell::new(repo)
        }
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("Presenter only running while view still exists")
    }

    fn open_clicked(&self) {
        self.view().open_repo_selector();
    }

    fn select_repo(&self, repo_dir: &Path) {
        use Config;

        let repo = match git2::Repository::open(&repo_dir) {
            Ok(repo) => repo,
            Err(err) => {
                self.view().handle_error(err);
                return;
            }
        };

        *self.repo.borrow_mut() = Rc::new(repo);

        Config::set_repo_dir(&repo_dir.to_string_lossy());
        
        // Reset everything
        self.start();
    }

    fn update_branches(&self) {
        let repo = self.repo.borrow();
        let mut names = vec![];
        for entry in repo.branches(Some(git2::BranchType::Local)).unwrap().into_iter() {
            let (branch, _) = entry.unwrap();
            let name = branch.name().unwrap().unwrap();
            names.push(name.to_string());
        }
        self.view().set_branches(Rc::clone(&repo), names);
    }

    fn start(&self) {
        let repo = self.repo.borrow();
        let path = repo.path().parent().unwrap().to_string_lossy();
        self.view().set_title(&path);

        // TODO: add directory watcher for new branches
        self.update_branches();
    }
}

pub struct MainWindow {
    presenter: MainPresenter<MainWindow>,
    header: MainWindowHeader,
    window: gtk::Window,
    sidebar_view: Rc<SidebarView>,
    branch_view: Rc<BranchView>,
    branches: RefCell<Vec<String>>
}

impl Window for MainWindow {}

impl MainViewable for MainWindow {
    fn with_repo(repo: git2::Repository) -> Rc<Self> {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Sourcepan");
        window.set_default_size(1024, 768);

        let header = MainWindow::create_header();
        window.set_titlebar(header.widget());

        let sidebar_view = MainWindow::create_sidebar();

        window.add(&sidebar_view.root);

        window.connect_delete_event(|_, _| {
            gtk::main_quit();
            Inhibit(false)
        });

        let repo = Rc::new(repo);
        let branch_view = BranchView::new(&window, Rc::clone(&repo));
        sidebar_view.root.add2(branch_view.widget());

        let view = view!(MainWindow {
            presenter: MainPresenter::new(Rc::clone(&repo)),
            header,
            window,
            sidebar_view: Rc::new(sidebar_view),
            branch_view,
            branches: RefCell::new(vec![])
        });
        
        view.header.open_button.connect_clicked(weak!(view => move |_| {
            if let Some(view) = view.upgrade() {
                view.presenter.open_clicked();
            } else {
                panic!("MainWindow open_button failed to resolve weak parent view");
            }
        }));

        {
            // let sidebar_view = &view.sidebar_view;
            view.sidebar_view.tree_view.connect_cursor_changed(weak!(view => move |_| {
                if let Some(view) = view.upgrade() {
                    let idxs = view.sidebar_view.tree_view.get_cursor().0.unwrap().get_indices();
                    if idxs.len() < 2 {
                        return;
                    }

                    let branch_idx = idxs[1];

                    if branch_idx >= 0 {
                        view.set_branch_by_index(branch_idx as usize);
                    }
                } else {
                    panic!("Sidebar not found in weak reference counter for tree selection");
                }
            }));
        }

        view.presenter.start();

        // Hack to make the pane be at 50% on first load
        view.branch_view.set_file_pane_to_half();

        view
    }

    fn handle_error(&self, error: impl fmt::Display) {
        let dialog = error.as_message_dialog(Some(&self.window));
        dialog.run();
        dialog.destroy();
    }
    
    fn set_branches(&self, repo: Rc<git2::Repository>, branches: Vec<String>) {
        self.branch_view.set_repo(repo);
        self.branch_view.set_branch(&branches[0]);
        self.sidebar_view.set_branches(&branches);

        *self.branches.borrow_mut() = branches;

        self.window.show_all();
    }

    fn set_branch_by_index(&self, index: usize) {
        self.branch_view.set_branch(&self.branches.borrow()[index])
    }

    fn open_repo_selector(&self) {
        let dialog = gtk::FileChooserNative::new(
            Some("Select Repository"),
            Some(&self.window),
            gtk::FileChooserAction::SelectFolder,
            Some("_Open"),
            Some("_Cancel"));
        
        let result = dialog.run();

        if result == -3 { // gtk::ResponseType::Accept.into() {
            if let Some(filename) = dialog.get_filename() {
                self.presenter.select_repo(&filename);
            }
        }
    }

    fn show(&self) {
        self.window.show_all();
    }

    fn set_title(&self, path: &str) {
        self.window.set_title(path);
        self.header.widget().set_title(path);
    }
}

struct MainWindowHeader {
    root: gtk::HeaderBar,
    open_button: gtk::Button
}

impl MainWindowHeader {
    fn widget(&self) -> &gtk::HeaderBar {
        &self.root
    }
}

struct SidebarView {
    tree_store: gtk::TreeStore,
    branch_iter: RefCell<gtk::TreeIter>,
    tree_view: gtk::TreeView,
    root: gtk::Paned
}

impl SidebarView {
    fn set_branches(&self, branches: &Vec<String>) {
        let branch_iter = self.tree_store.insert_with_values(None, None, &[0], &[&"Branches"]);

        for branch in branches {
            self.tree_store.insert_with_values(Some(&branch_iter), None, &[0], &[&branch]);
        }
        
        {
            let iter = self.branch_iter.borrow();
            self.tree_store.swap(&branch_iter, &*iter);
            self.tree_store.remove(&iter);
        }
        
        *self.branch_iter.borrow_mut() = branch_iter;
        self.tree_view.expand_all();
    }
}

impl MainWindow {
    fn create_sidebar() -> SidebarView {
        let tree_store = gtk::TreeStore::new(&[
            String::static_type()
        ]);

        let branch_iter = tree_store.insert_with_values(None, None, &[0], &[&"Branches"]);
        tree_store.insert_with_values(Some(&branch_iter), None, &[0], &[&"master"]);

        let tree_view = gtk::TreeView::new();
        tree_view.set_model(&tree_store);
        tree_view.set_headers_visible(false);

        let renderer_name = gtk::CellRendererText::new();
        let column_name = gtk::TreeViewColumn::new();
        column_name.pack_start(&renderer_name, true);
        column_name.set_resizable(false);
        column_name.add_attribute(&renderer_name, "text", 0);
        tree_view.append_column(&column_name);
        tree_view.expand_all();

        let root = gtk::Paned::new(gtk::Orientation::Horizontal);
        root.set_vexpand(true);
        root.set_hexpand(true);
        root.add1(&tree_view);

        // main_box.attach(&sidebar, 0, 0, 1, 1);
        // main_box.attach(&stack, 1, 0, 1, 1);
        
        // window.add(&main_box);
        // window.add(&stack);
        
        SidebarView {
            tree_store,
            branch_iter: RefCell::new(branch_iter),
            tree_view,
            root
        }
    }

    fn create_header() -> MainWindowHeader {
        let header_bar = gtk::HeaderBar::new();
        header_bar.set_title("Sourcepan");
        header_bar.set_show_close_button(true);

        // let commit_button = gtk::Button::new_with_label("Commit");

        // let action_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        // action_box.get_style_context().unwrap().add_class("linked");

        // let pull_button = gtk::Button::new_with_label("Pull");
        // let push_button = gtk::Button::new_with_label("Push");
        // let fetch_button = gtk::Button::new_with_label("Fetch");

        // action_box.add(&pull_button);
        // action_box.add(&push_button);
        // action_box.add(&fetch_button);

        // let action_box2 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        // action_box2.get_style_context().unwrap().add_class("linked");

        // let branch_button = gtk::Button::new_with_label("Branch");
        // let merge_button = gtk::Button::new_with_label("Merge");

        // action_box2.add(&branch_button);
        // action_box2.add(&merge_button);

        // header_bar.pack_start(&commit_button);
        // header_bar.pack_start(&action_box);
        // header_bar.pack_start(&action_box2);

        // let stash_button = gtk::Button::new_with_label("Stash");
        // header_bar.pack_start(&stash_button);

        let settings_button = gtk::Button::new_with_label("Preferences");
        header_bar.pack_end(&settings_button);

        let open_button = gtk::Button::new_with_label("Open");
        header_bar.pack_end(&open_button);

        MainWindowHeader {
            root: header_bar,
            open_button: open_button
        }
    }
}

#[derive(Debug)]
pub struct TreeItem {
    id: git2::Oid,
    path: String,
    delta: git2::Delta,
    is_selected: bool
}
