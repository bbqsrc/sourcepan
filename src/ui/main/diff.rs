use std::cell::RefCell;

use git2;
use gtk::prelude::*;
use gtk;
use gdk;

trait DiffViewable {

}

trait DiffChunkViewable {

}

#[allow(dead_code)]
pub struct DiffChunkView {
    list_store: gtk::ListStore,
    label: gtk::Label,
    tree: gtk::TreeView,
    root: gtk::Box
}

impl DiffChunkViewable for DiffChunkView {

}

pub struct DiffView {
    root: gtk::ScrolledWindow,
    container: gtk::Box,
    files: RefCell<Vec<DiffFileView>>
}

impl DiffView {
    pub fn new() -> DiffView {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
        container.get_style_context().unwrap().add_class("diff-container");

        let root = gtk::ScrolledWindow::new(None, None);
        root.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        root.add(&container);
        // root.get_style_context().unwrap().add_class("diff-container");

        DiffView {
            root,
            container,
            files: RefCell::new(vec![])
        }
    }

    pub fn set_diff(&self, diff: git2::Diff) {
        for child in self.container.get_children() {
            self.container.remove(&child);
        }

        let mut views = vec![];

        for (n, d) in diff.deltas().enumerate() {
            let mut patch = git2::Patch::from_diff(&diff, n).unwrap().unwrap();
            let path = d.new_file().path().unwrap().to_string_lossy();

            let view = DiffFileView::new(patch, &path);
            self.container.add(view.widget());
            views.push(view);
        }

        *self.files.borrow_mut() = views;

        self.container.show_all();
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }
}

#[allow(dead_code)]
struct DiffFileView {
    root: gtk::Box,
    label: gtk::Label
}

impl DiffFileView {
    pub fn new(patch: git2::Patch, path: &str) -> DiffFileView {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.get_style_context().unwrap().add_class("file-border");
        let label = gtk::Label::new(path);
        label.set_xalign(0.0);
        label.get_style_context().unwrap().add_class("file-label");

        root.add(&label);

        for hunk_idx in 0..patch.num_hunks() {
            // TODO: multiple tree views for the hunks
            let hunk = patch.hunk(hunk_idx).unwrap().0;
            let header = ::std::str::from_utf8(hunk.header()).unwrap();

            let chunk_view = DiffChunkView::new(&patch, header, hunk_idx);
            root.add(chunk_view.widget());
        }

        DiffFileView {
            root,
            label
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

impl DiffChunkView {
    pub fn new(patch: &git2::Patch, header: &str, hunk_idx: usize) -> DiffChunkView {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.get_style_context().unwrap().add_class("tree-border");
        let label = gtk::Label::new(header.trim());
        label.set_xalign(0.0);
        label.get_style_context().unwrap().add_class("diff-label");
        let tree = gtk::TreeView::new();

        tree.get_style_context().unwrap().add_class("monospace");
        tree.get_selection().set_mode(gtk::SelectionMode::Multiple);

        let list_store = gtk::ListStore::new(&[
            gdk::RGBA::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type(),
            gdk::RGBA::static_type()
        ]);

        root.add(&label);
        root.add(&tree);

        fn append_column(tree: &gtk::TreeView, id: i32, title: &str, is_colored: bool) -> gtk::TreeViewColumn {
            let column = gtk::TreeViewColumn::new();
            let cell = gtk::CellRendererText::new();

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
        
        append_column(&tree, 1, "Old", false);
        append_column(&tree, 2, "New", false);
        append_column(&tree, 3, "Origin", true);
        append_column(&tree, 4, "Line", true);

        tree.set_headers_visible(false);
        
        tree.set_model(&list_store);

        for i in 0..patch.num_lines_in_hunk(hunk_idx).unwrap() {
            let line = patch.line_in_hunk(hunk_idx, i).unwrap();

            let rgba = match line.origin() {
                '>' | '+' => gdk::RGBA {
                    red: 0.851,
                    green: 0.925,
                    blue: 0.812,
                    alpha: 1.0
                },
                '<' | '-' => gdk::RGBA {
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
                &line.origin().to_string(),
                &::std::str::from_utf8(line.content()).unwrap_or("<unknown>").trim_right(),
                &gdk::RGBA { red: 0.9, green: 0.9, blue: 0.9, alpha: 1.0 }
            ]);
        }
        
        root.show_all();

        DiffChunkView {
            list_store,
            label,
            tree,
            root
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
