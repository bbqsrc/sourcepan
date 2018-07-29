mod branch;
mod history;

use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::path::Path;
use std::error;

use git2;
use gtk::prelude::*;
use gtk;

use ui::Window;
use ui::main::branch::{BranchViewable, BranchView};

#[derive(Debug)]
pub struct CommitInfo {
    pub id: git2::Oid,
    pub summary: String,
    pub short_id: String,
    pub author: String,
    pub commit_date: String
}

impl CommitInfo {
    pub fn uncommitted_sentinel() -> CommitInfo {
        CommitInfo {
            id: git2::Oid::zero(),
            summary: "Uncommitted changes".into(),
            short_id: "*".into(),
            author: "*".into(),
            commit_date: "*".into()
        }
    }

    pub fn is_sentinel(&self) -> bool {
        self.id == git2::Oid::zero() && self.summary == "Uncommitted changes"
    }
}

struct MainPresenter<V> {
    view: RefCell<Weak<V>>,
    repo: RefCell<Rc<git2::Repository>>
}

pub trait MainViewable {
    fn with_repo(repo: git2::Repository) -> Rc<Self>;
    fn set_branches(&self, repo: Rc<git2::Repository>, branches: Vec<String>);
    fn show(&self);
    fn set_title(&self, path: &str);
    fn open_repo_selector(&self);
    fn handle_error<T: error::Error>(&self, error: T);
}

impl<V: MainViewable> MainPresenter<V> {
    fn new(repo: git2::Repository) -> MainPresenter<V> {
        MainPresenter {
            view: RefCell::new(Weak::new()),
            repo: RefCell::new(Rc::new(repo))
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
    branch_views: RefCell<Vec<Rc<BranchView>>>
}

impl Window for MainWindow {}

impl MainViewable for MainWindow {
    fn with_repo(repo: git2::Repository) -> Rc<Self> {
        let (window, header) = MainWindow::create();

        let view = view!(MainWindow {
            presenter: MainPresenter::new(repo),
            header: header,
            window: window,
            branch_views: RefCell::new(vec![])
        });
        
        view.header.open_button.connect_clicked(weak!(view => move |_| {
            if let Some(view) = view.upgrade() {
                view.presenter.open_clicked();
            } else {
                panic!("MainWindow open_button failed to resolve weak parent view");
            }
        }));

        view.presenter.start();

        view
    }

    fn handle_error<T: error::Error>(&self, error: T) {
        let dialog = gtk::MessageDialog::new(
            Some(&self.window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Error,
            gtk::ButtonsType::Close,
            &format!("{}", error)
        );

        dialog.set_title("Error");
        dialog.run();

        // Once you press close, main loop returns control and closes the window.
        dialog.destroy();
    }
    
    fn set_branches(&self, repo: Rc<git2::Repository>, branches: Vec<String>) {
        let stack = MainWindow::create_sidebar(&self.window);
        let mut branch_views = vec![];

        for name in branches.into_iter() {
            let branch_view = BranchView::new(Rc::clone(&repo), name.to_string());
            &stack.add_titled(branch_view.widget(), &format!("branch-{}", name), &name);
            branch_views.push(branch_view);
        }

        *self.branch_views.borrow_mut() = branch_views;

        self.window.show_all();
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

impl MainWindow {
    fn create_sidebar(window: &gtk::Window) -> gtk::Stack {
        // Remove the entire grid and recreate if exists.
        if let Some(child) = window.get_children().first() {
            if let Some(name) = <gtk::Widget as WidgetExt>::get_name(child) {
                if name == "GtkGrid" {
                    window.remove(child);
                }
            }
        }

        let sidebar = gtk::StackSidebar::new();
        let main_box = gtk::Grid::new();

        let stack = gtk::Stack::new();
        stack.set_vexpand(true);
        stack.set_hexpand(true);
        sidebar.set_stack(&stack);

        main_box.attach(&sidebar, 0, 0, 1, 1);
        main_box.attach(&stack, 1, 0, 1, 1);
        
        window.add(&main_box);
        
        stack
    }

    fn create_header() -> MainWindowHeader {
        let header_bar = gtk::HeaderBar::new();
        header_bar.set_title("Sourcepan");
        header_bar.set_show_close_button(true);

        let commit_button = gtk::Button::new_with_label("Commit");

        let action_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        action_box.get_style_context().unwrap().add_class("linked");

        let pull_button = gtk::Button::new_with_label("Pull");
        let push_button = gtk::Button::new_with_label("Push");
        let fetch_button = gtk::Button::new_with_label("Fetch");

        action_box.add(&pull_button);
        action_box.add(&push_button);
        action_box.add(&fetch_button);

        let action_box2 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        action_box2.get_style_context().unwrap().add_class("linked");

        let branch_button = gtk::Button::new_with_label("Branch");
        let merge_button = gtk::Button::new_with_label("Merge");

        action_box2.add(&branch_button);
        action_box2.add(&merge_button);

        header_bar.pack_start(&commit_button);
        header_bar.pack_start(&action_box);
        header_bar.pack_start(&action_box2);

        let stash_button = gtk::Button::new_with_label("Stash");
        header_bar.pack_start(&stash_button);

        let settings_button = gtk::Button::new_with_label("Preferences");
        header_bar.pack_end(&settings_button);

        let open_button = gtk::Button::new_with_label("Open");
        header_bar.pack_end(&open_button);

        MainWindowHeader {
            root: header_bar,
            open_button: open_button
        }
    }

    fn create() -> (gtk::Window, MainWindowHeader) {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Sourcepan");
        window.set_default_size(1024, 768);

        let header = MainWindow::create_header();
        window.set_titlebar(header.widget());

        let _ = MainWindow::create_sidebar(&window);

        window.connect_delete_event(|_, _| {
            gtk::main_quit();
            Inhibit(false)
        });

        (window, header)
    }
}
