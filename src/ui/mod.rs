#![macro_use]

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

pub trait Window {}

pub mod init;
pub mod main;
