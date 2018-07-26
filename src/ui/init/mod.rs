use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::path::Path;

use git2;
use gtk::prelude::*;
use gtk;

use ui::main::{MainViewable, MainWindow};

struct InitPresenter<V: InitViewable> {
    view: RefCell<Weak<V>>
}

impl<V: InitViewable> InitPresenter<V> {
    fn new() -> InitPresenter<V> {
        InitPresenter { view: RefCell::new(Weak::new()) }
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("Presenter only running while view still exists")
    }

    fn click_open(&self) {
        self.view().open_repo_selector();
    }

    fn select_repo(&self, repo_dir: &Path) {
        use Config;

        let repo = git2::Repository::open(&repo_dir).unwrap();
        self.view().open_main_window_with(repo);

        Config::set_repo_dir(&repo_dir.to_string_lossy());
    }
}

pub trait InitViewable {
    fn new() -> Rc<Self>;
    fn show(&self);
    fn hide(&self);
    fn open_repo_selector(&self);
    fn open_main_window_with(&self, repo: git2::Repository);
}

pub struct InitWindow {
    presenter: InitPresenter<InitWindow>,
    window: gtk::Window
}

impl InitWindow {
    fn create() -> (gtk::Window, gtk::Button) {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Sourcepan");
        window.set_default_size(600, 64);

        let open_button = gtk::Button::new_with_label("Open");
        window.add(&open_button);

        window.connect_delete_event(|_, _| {
            gtk::main_quit();
            Inhibit(false)
        });

        (window, open_button)
    }
}

impl InitViewable for InitWindow {
    fn new() -> Rc<InitWindow> {
        let (window, open_button) = InitWindow::create();

        let view = Rc::new(InitWindow {
            presenter: InitPresenter::new(),
            window: window
        });

        *view.presenter.view.borrow_mut() = Rc::downgrade(&view);

        let weak_view = view.clone();//Rc::downgrade(&view);
        open_button.connect_clicked(move |_| {
            // if let Some(v) = weak_view.upgrade() {
                weak_view.presenter.click_open();
            // }
        });

        view
    }

    fn show(&self) {
        self.window.show_all();
    }

    fn hide(&self) {
        self.window.hide();
    }

    fn open_repo_selector(&self) {
        let dialog = gtk::FileChooserNative::new(
            Some("Select Repository"),
            Some(&self.window),
            gtk::FileChooserAction::SelectFolder,
            Some("_Open"),
            Some("_Cancel"));
        
        let result = dialog.run();

        if result == gtk::ResponseType::Accept.into() {
            if let Some(filename) = dialog.get_filename() {
                self.presenter.select_repo(&filename);
            }
        }
    }

    fn open_main_window_with(&self, repo: git2::Repository) {
        self.hide();
        let main_window = MainWindow::with_repo(repo);
        main_window.show();
    }
}
