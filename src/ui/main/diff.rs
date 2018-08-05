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

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::{Arc, RwLock};

use git2;
use gtk::prelude::*;
use gtk;
use gdk;
use pango;

use ui;
use ui::Parent;

const NO_NL_STR: &'static str = "No newline at end of file";

pub struct DiffView {
    root: gtk::ScrolledWindow,
    container: gtk::Box,
    files: RefCell<Vec<Rc<DiffFileView>>>
}

impl DiffView {
    pub fn new() -> DiffView {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
        container.get_style_context().unwrap().add_class("diff-container");

        let root = gtk::ScrolledWindow::new(None, None);
        root.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        root.add(&container);

        DiffView {
            root,
            container,
            files: RefCell::new(vec![])
        }
    }

    pub fn set_diff(view: &Rc<DiffView>, diff: git2::Diff, context: DiffContext) {
        for child in view.container.get_children() {
            view.container.remove(&child);
        }

        let mut views = vec![];

        for (n, d) in diff.deltas().enumerate() {
            let mut patch = git2::Patch::from_diff(&diff, n).unwrap().unwrap();
            let path = d.new_file().path().unwrap().to_string_lossy();

            let file_view = DiffFileView::new(Rc::downgrade(&view), context, patch, &path);
            view.container.add(file_view.widget());
            views.push(file_view);
        }

        *view.files.borrow_mut() = views;

        view.container.show_all();
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }
}

#[allow(dead_code)]
pub struct DiffFileView {
    root: gtk::Box,
    label: gtk::Label,
    parent: Weak<DiffView>,
    children: RefCell<Vec<Rc<DiffChunkView>>>
}

impl ui::Parent for DiffFileView {
    type View = DiffView;

    fn parent(&self) -> Option<Rc<DiffView>> {
        self.parent.upgrade()
    }
}

impl DiffFileView {
    pub fn new(parent: Weak<DiffView>, context: DiffContext, patch: git2::Patch, path: &str) -> Rc<DiffFileView> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.get_style_context().unwrap().add_class("file-border");
        
        let label = gtk::Label::new(path);
        label.set_xalign(0.0);
        label.set_ellipsize(pango::EllipsizeMode::Middle);
        label.get_style_context().unwrap().add_class("file-label");

        root.add(&label);

        let view = Rc::new(DiffFileView {
            root,
            label,
            parent,
            children: RefCell::new(vec![])
        });

        for hunk_idx in 0..patch.num_hunks() {
            let hunk = patch.hunk(hunk_idx).unwrap().0;
            let header = ::std::str::from_utf8(hunk.header()).unwrap();

            let chunk_view = DiffChunkView::new(Rc::downgrade(&view), context, &patch, header, hunk_idx);
            view.root.add(chunk_view.widget());
            view.children.borrow_mut().push(chunk_view);
        }

        view
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

trait HumanDiffLineExt<'a> {
    fn origin_human(&self) -> char;
    fn content_str(&self) -> Option<&str>;
}

impl<'a> HumanDiffLineExt<'a> for git2::DiffLine<'a> {
    fn origin_human(&self) -> char {
        let ch = self.origin();
        match ch {
            '=' | '>' | '<' => '\\',
            _ => ch
        }
    }

    fn content_str(&self) -> Option<&str> {
        match ::std::str::from_utf8(self.content()) {
            Ok(v) => {
                // Check for weird NL string
                if v.trim_right().ends_with(NO_NL_STR) {
                    Some(NO_NL_STR)
                } else {
                    Some(v.trim_right())
                }
            },
            Err(_) => None
        }
    }
                
}

#[allow(dead_code)]
pub struct DiffChunkView {
    presenter: DiffChunkPresenter<DiffChunkView>,
    list_store: gtk::ListStore,
    label: gtk::Label,
    primary_button: gtk::Button,
    lines_tree: gtk::TreeView,
    count_tree: gtk::TreeView,
    root: gtk::Box,
    parent: Weak<DiffFileView>
}

impl ui::Parent for DiffChunkView {
    type View = DiffFileView;

    fn parent(&self) -> Option<Rc<DiffFileView>> {
        self.parent.upgrade()
    }
}

trait DiffChunkViewable {
    fn show_revert_selected_lines(&self);
    fn show_unstage_selected_lines(&self);
    fn show_stage_selected_lines(&self);
    fn show_revert_all_lines(&self);
    fn show_stage_all_lines(&self);
    fn show_unstage_all_lines(&self);
    fn show_incontiguous_selection_error(&self);
    fn on_primary_button_clicked(&self);
    fn on_selected_lines(&self, rows: &[usize]);
}

#[derive(Copy, Clone, Debug)]
pub enum DiffContext {
    Committed,
    Staged,
    Working
}

struct DiffChunkPresenter<V: DiffChunkViewable> {
    view: RefCell<Weak<V>>,
    context: DiffContext
}

impl<V: DiffChunkViewable> DiffChunkPresenter<V> {
    pub fn new(context: DiffContext) -> DiffChunkPresenter<V> {
        DiffChunkPresenter { 
            view: RefCell::new(Weak::new()),
            context
        }
    }

    pub fn start(&self) {
        self.handle_on_selected_lines(&[]);
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow().upgrade().expect("The view exists")
    }

    fn handle_on_primary_button_clicked(&self) {
        
    }

    fn handle_on_selected_lines(&self, lines: &[usize]) {
        let has_selection = lines.len() > 0;
        let is_contiguous = lines.windows(2).all(|s| s[1] == s[0] + 1);

        match self.context {
            DiffContext::Committed => {
                if has_selection && is_contiguous {
                    self.view().show_revert_selected_lines()
                } else if !has_selection {
                    self.view().show_revert_all_lines()
                } else {
                    self.view().show_incontiguous_selection_error()
                }
            }
            DiffContext::Staged => {
                if has_selection && is_contiguous {
                    self.view().show_unstage_selected_lines()
                } else if !has_selection {
                    self.view().show_unstage_all_lines()
                } else {
                    self.view().show_incontiguous_selection_error()
                }
            }
            DiffContext::Working => {
                if has_selection && is_contiguous {
                    self.view().show_stage_selected_lines()
                } else if !has_selection {
                    self.view().show_stage_all_lines()
                } else {
                    self.view().show_incontiguous_selection_error()
                }
            }
        }
    }
}

impl DiffChunkViewable for DiffChunkView {
    fn show_revert_selected_lines(&self) {
        self.primary_button.set_sensitive(true);
        self.primary_button.set_label("Revert Selected Lines");
        self.primary_button.show_all();
    }
    
    fn show_unstage_selected_lines(&self) {
        self.primary_button.set_sensitive(true);
        self.primary_button.set_label("Unstage Selected Lines");
        self.primary_button.show_all();
    }
    
    fn show_stage_selected_lines(&self) {
        self.primary_button.set_sensitive(true);
        self.primary_button.set_label("Stage Selected Lines");
        self.primary_button.show_all();
    }
    
    fn show_revert_all_lines(&self) {
        self.primary_button.set_sensitive(true);
        self.primary_button.set_label("Revert All Lines");
        self.primary_button.show_all();
    }
    
    fn show_stage_all_lines(&self) {
        self.primary_button.set_sensitive(true);
        self.primary_button.set_label("Stage All Lines");
        self.primary_button.show_all();
    }
    
    fn show_unstage_all_lines(&self) {
        self.primary_button.set_sensitive(true);
        self.primary_button.set_label("Unstage All Lines");
        self.primary_button.show_all();
    }

    fn show_incontiguous_selection_error(&self) {
        self.primary_button.set_sensitive(false);
        self.primary_button.set_label("Incontiguous selection");
        self.primary_button.show_all();
    }

    fn on_primary_button_clicked(&self) {
        self.presenter.handle_on_primary_button_clicked();
    }

    fn on_selected_lines(&self, rows: &[usize]) {
        self.presenter.handle_on_selected_lines(&rows);
    }
}

impl DiffChunkView {
    pub fn new(parent: Weak<DiffFileView>, context: DiffContext, patch: &git2::Patch, header: &str, hunk_idx: usize) -> Rc<DiffChunkView> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.get_style_context().unwrap().add_class("tree-border");

        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header_box.get_style_context().unwrap().add_class("diff-header");

        let label = gtk::Label::new(header.trim());
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.get_style_context().unwrap().add_class("diff-label");
        label.set_ellipsize(pango::EllipsizeMode::Middle);
        
        let button = gtk::Button::new_with_label("");
        button.get_style_context().unwrap().add_class("small-button");

        header_box.add(&label);
        header_box.add(&button);

        root.add(&header_box);

        let count_tree = gtk::TreeView::new();
        count_tree.get_style_context().unwrap().add_class("line-count");
        count_tree.get_selection().set_mode(gtk::SelectionMode::None);

        let lines_tree = gtk::TreeView::new();
        lines_tree.get_style_context().unwrap().add_class("monospace");
        lines_tree.get_selection().set_mode(gtk::SelectionMode::Multiple);

        let first_clicked: Arc<RwLock<Option<gtk::TreePath>>> = Arc::new(RwLock::new(None)); 

        lines_tree.connect_button_press_event(clone!(first_clicked => move |tree, event| {
            let (x, y) = event.get_position();
            let res = match tree.get_path_at_pos(x as i32, y as i32) {
                Some(v) => v,
                None => { return gtk::Inhibit(false); }
            };
            let path = match res.0 {
                Some(v) => v,
                None => { return gtk::Inhibit(false); }
            };

            let mut guard = first_clicked.write().unwrap();
            *guard = Some(path);
            gtk::Inhibit(false)
        }));

        lines_tree.connect_motion_notify_event(clone!(first_clicked => move |tree, event| {
            let lock = first_clicked.read().unwrap();
            
            let tree_path = match &*lock {
                Some(v) => v,
                None => {
                    return gtk::Inhibit(false);
                }
            };

            let (x, y) = event.get_position();
            let res = match tree.get_path_at_pos(x as i32, y as i32) {
                Some(v) => v,
                None => { return gtk::Inhibit(false); }
            };
            let new_path = match res.0 {
                Some(v) => v,
                None => { return gtk::Inhibit(false); }
            };

            let selection = tree.get_selection();
            selection.unselect_all();
            selection.select_range(&tree_path, &new_path);
            gtk::Inhibit(false)
        }));

        lines_tree.connect_button_release_event(clone!(first_clicked => move |_, _| {
            let mut guard = first_clicked.write().unwrap();
            *guard = None;

            gtk::Inhibit(false)
        }));

        let list_store = gtk::ListStore::new(&[
            gdk::RGBA::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type(),
            gdk::RGBA::static_type()
        ]);

        let trees_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        trees_box.add(&count_tree);

        let scroller = gtk::ScrolledWindow::new(None, None);
        scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
        scroller.add(&lines_tree);
        scroller.set_hexpand(true);
        trees_box.add(&scroller);

        root.add(&trees_box);

        fn append_column(tree: &gtk::TreeView, id: i32, title: &str, is_colored: bool) -> gtk::TreeViewColumn {
            let column = gtk::TreeViewColumn::new();
            let cell = gtk::CellRendererText::new();

            if id <= 2 {
                cell.set_alignment(0.5, 0.5);
            }

            column.pack_start(&cell, true);
            column.set_resizable(true);
            column.set_title(title);
            column.add_attribute(&cell, "text", id);
            if is_colored {
                column.add_attribute(&cell, "background-rgba", 0);
            } else {
                column.add_attribute(&cell, "background-rgba", 5);
                column.set_min_width(24);
            }
            tree.append_column(&column);

            column
        }
        
        let column = gtk::TreeViewColumn::new();
        let cell = gtk::CellRendererText::new();
        column.pack_start(&cell, true);
        column.set_visible(false);
        
        append_column(&count_tree, 1, "Old", false);
        append_column(&count_tree, 2, "New", false);
        append_column(&lines_tree, 3, "Origin", true);
        append_column(&lines_tree, 4, "Line", true);

        count_tree.set_headers_visible(false);
        lines_tree.set_headers_visible(false);
        
        count_tree.set_model(&list_store);
        lines_tree.set_model(&list_store);

        for i in 0..patch.num_lines_in_hunk(hunk_idx).unwrap() {
            let line = patch.line_in_hunk(hunk_idx, i).unwrap();

            let rgba = match line.origin() {
                '<' | '+' => gdk::RGBA {
                    red: 0.851,
                    green: 0.925,
                    blue: 0.812,
                    alpha: 1.0
                },
                '>' | '-' => gdk::RGBA {
                    red: 0.918,
                    green: 0.835,
                    blue: 0.835,
                    alpha: 1.0
                },
                _ => gdk::RGBA::white()
            };

            (&list_store).insert_with_values(None, &[0, 1, 2, 3, 4, 5], &[
                &rgba,
                &line.old_lineno().map(|x| x.to_string()).unwrap_or("".to_string()),
                &line.new_lineno().map(|x| x.to_string()).unwrap_or("".to_string()),
                &line.origin_human().to_string(),
                &line.content_str().unwrap_or("<unknown>"),
                &gdk::RGBA { red: 0.95, green: 0.95, blue: 0.95, alpha: 1.0 }
            ]);
        }
        
        root.show_all();

        let presenter = DiffChunkPresenter::new(context);

        let view = view!(DiffChunkView {
            presenter,
            list_store,
            label,
            primary_button: button,
            count_tree,
            lines_tree,
            root,
            parent
        });

        view.lines_tree.connect_focus_out_event(weak!(view => move |_, _| {
            let view = try_upgrade!(view, gtk::Inhibit(false));

            gtk::idle_add(weak!(view => move || {
                let view = try_upgrade!(view, gtk::Continue(false));
                let tree = &view.lines_tree;

                let top_level = match tree.get_toplevel() {
                    Some(v) => v,
                    None => {
                        // TODO: warning that this isn't in window; shoudl not be possible
                        return gtk::Continue(false);
                    }
                };

                let window = match top_level.downcast::<gtk::Window>() {
                    Ok(v) => v,
                    Err(_) => {
                        // TODO: warning that this isn't in window; shoudl not be possible
                        return gtk::Continue(false);
                    }
                };

                let focused_widget = match window.get_focus() {
                    Some(v) => v,
                    None => {
                        tree.get_selection().unselect_all();
                        return gtk::Continue(false);
                    }
                };

                if !focused_widget.is_ancestor(view.widget()) {
                    tree.get_selection().unselect_all();
                }

                gtk::Continue(false)
            }));
            
            gtk::Inhibit(false)
        }));

        view.lines_tree.get_selection().connect_changed(weak!(view => move |selection| {
            // println!("Selection changed");
            let view = try_upgrade!(view);

            // println!("View lives");

            let rows: Vec<_> = selection.get_selected_rows().0
                .into_iter()
                .map(|x| x.get_indices()[0] as usize)
                .collect();
                
            view.on_selected_lines(&rows);
        }));

        view.primary_button.connect_clicked(weak!(view => move |_| {
            let view = try_upgrade!(view);
            view.on_primary_button_clicked();
        }));

        view.presenter.start();

        view
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
