#[macro_use]
extern crate lazy_static;
extern crate sdl2;
use sdl2::*;
use sdl2::event::Event;

use std::collections::{HashMap, LinkedList};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

type SdlLambda = FnMut(&mut Sdl, &mut HashMap<u32, video::Window>) + Send;
type SdlCreateWindow = FnMut(&mut Sdl, &mut VideoSubsystem) -> Option<video::Window> + Send;
type SdlHandleEvent = FnMut(&mut Sdl, &mut HashMap<u32, video::Window>, &Event) -> bool + Send;

pub enum Sdl2Message {
    Lambda(Box<SdlLambda>),
    CreateWindow(Box<SdlCreateWindow>, mpsc::Sender<Option<u32>>),
    HandleEvent(Box<SdlHandleEvent>, mpsc::Sender<()>),
    Exit
}

use Sdl2Message::*;

fn sdl_handler(rx: mpsc::Receiver<Sdl2Message>) {
    let mut sdl_context = sdl2::init().unwrap();
    let mut video = sdl_context.video().unwrap();
    let mut events = sdl_context.event_pump().unwrap();

    let mut windows = HashMap::new();
    let mut unhandled_events = LinkedList::new(); // really, we need to drop old events at some point
    for message in rx {
        match message {
            Lambda(mut lambda) => lambda(&mut sdl_context, &mut windows),
            CreateWindow(mut create_window, tx) => {
                let window_id;
                if let Some(window) = create_window(&mut sdl_context, &mut video) {
                    let id = window.id();
                    windows.insert(id, window);
                    window_id = Some(id);
                } else {
                    window_id = None;
                }

                // Send the Window ID back to the requesting thread
                // -----------------------------------------------------------------
                // if send fails, sdl2_mt can panic or print an error or do nothing.
                // panicking in a library is a bad plan.
                // printing errors from a library needs to be configurable.
                //   if printing is configurable, might as well make panicking an option too.
                // for now, sdl2_mt will do nothing.
                let _ = tx.send(window_id);
            },
            HandleEvent(mut handle_event, tx) => {
                let len = unhandled_events.len(); // should be O(1) according to docs
                for _ in 0..len {
                    let event = unhandled_events.pop_front().unwrap(); //we're within the length of the list
                    if !handle_event(&mut sdl_context, &mut windows, &event) {
                        // if the event was unhandled, put it back on the list
                        unhandled_events.push_back(event);
                    }
                }

                for event in events.poll_iter() {
                    if !handle_event(&mut sdl_context, &mut windows, &event) {
                        // if the event was unhandled, add it to the list
                        unhandled_events.push_back(event);
                    }
                }
                
                // Synchronize with calling thread to prevent unbounded HandleEvents messages queueing up
                // Same logic as above regarding errors
                let _ = tx.send(()); 
            }
            Exit => break
        }
    }
}

#[derive(Clone)]
pub struct Sdl2Mt(mpsc::Sender<Sdl2Message>);

pub type UiThreadExited = ();

#[inline]
fn map_ute<T>(_: T) -> UiThreadExited {
    ()
}

impl Sdl2Mt {
    // Executes a window_creator function that accepts &mut VideoSubsystem
    // and returns an Option<Window>. If Some(window), it will be
    // added to a HashMap, hashing on the window's ID, which will
    // then be returned here. If None, None will be returned here.
    //
    // This function executes synchronously. It will block until the
    // window_creator function has completed.
    pub fn create_window(&self, window_creator: Box<SdlCreateWindow>) -> Result<Option<u32>, UiThreadExited> {
        let (tx, rx) = mpsc::channel();
        self.0.send(CreateWindow(window_creator, tx)).map_err(map_ute)?;
        rx.recv().map_err(map_ute)
    }

    /// Executes a lambda function on the UI thread
    /// Either succeeds or the channel is closed and it returns a `SendError`
    //
    // This function executes asynchronously. It will *not* block the calling thread.
    pub fn run_on_ui_thread(&self, lambda: Box<SdlLambda>) -> Result<(), UiThreadExited> {
        self.0.send(Lambda(lambda)).map_err(map_ute)
    }

    // Executes an event_handler function.
    //
    // This function executes synchronously. It will block until the
    // event_handler function has completed.
    pub fn handle_ui_events(&self, event_handler: Box<SdlHandleEvent>) -> Result<(), UiThreadExited> {
        let (tx, rx) = mpsc::channel();
        self.0.send(HandleEvent(event_handler, tx)).map_err(map_ute)?;
        rx.recv().map_err(map_ute)
    }

    // Kills the UI thread. Not strictly necessary if the program will be terminated anyways.
    pub fn exit(&self) -> Result<(), UiThreadExited> {
        self.0.send(Exit).map_err(map_ute)
    }
}

lazy_static! {
    static ref MT_HANDLE: Arc<Mutex<Sdl2Mt>> = {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || sdl_handler(rx));
        let handle = Sdl2Mt(tx);
        Arc::new(Mutex::new(handle))
    };
}

pub fn init() -> Sdl2Mt {
    let handle = (*MT_HANDLE).clone();
    let locked = handle.lock().unwrap();
    let new_handle = locked.clone();
    new_handle
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn double_init() {
        let a = init();
        let b = init();
        sleep(Duration::from_millis(250));
        a.run_on_ui_thread(Box::new(|_, _| {})).unwrap();
        sleep(Duration::from_millis(250));
        b.run_on_ui_thread(Box::new(|_, _| {})).unwrap();
        sleep(Duration::from_millis(250));
    }
}