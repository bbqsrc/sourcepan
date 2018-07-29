#![macro_use]

use gtk;

macro_rules! view {
    ($view:expr) => {
        {
            let view = Rc::new($view);
            *view.presenter.view.borrow_mut() = Rc::downgrade(&view);
            view
        }
    };
}

// macro_rules! clone {
//     (@param _) => ( _ );
//     (@param $x:ident) => ( $x );
//     ($($n:ident),+ => move || $body:expr) => (
//         {
//             $( let $n = $n.clone(); )+
//             move || $body
//         }
//     );
//     ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
//         {
//             $( let $n = $n.clone(); )+
//             move |$(clone!(@param $p),)+| $body
//         }
//     );
// }

macro_rules! weak {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = Rc::downgrade(&$n); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = Rc::downgrade(&$n); )+
            move |$(weak!(@param $p),)+| $body
        }
    );
}

use std::fmt;
use gtk::prelude::*;

pub trait AsMessageDialog : fmt::Display {
    fn as_message_dialog<W: IsA<gtk::Window>>(&self, parent: Option<&W>) -> gtk::MessageDialog;
}

impl<T: fmt::Display> AsMessageDialog for T {
    fn as_message_dialog<W: IsA<gtk::Window>>(&self, parent: Option<&W>) -> gtk::MessageDialog {
        let dialog = gtk::MessageDialog::new(
            parent,
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Error,
            gtk::ButtonsType::Close,
            &format!("{}", self)
        );

        dialog.set_title("Error");
        dialog
    }
}

pub trait Window {}

pub mod init;
pub mod main;
