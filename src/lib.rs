extern crate sdl2;
pub use sdl2::*;
use event::Event;

use std::collections::{HashMap, LinkedList};
use std::sync::mpsc;
use std::thread;
use std::thread::sleep;
use std::time::Duration;

type SdlLambda = FnMut(&mut Sdl, &mut HashMap<u32, render::WindowCanvas>) + Send;
type SdlCreateWindow = FnMut(&mut Sdl, &mut VideoSubsystem) -> Option<render::WindowCanvas> + Send;
type SdlHandleEvent = FnMut(&mut Sdl, &mut HashMap<u32, render::WindowCanvas>, &Event) -> bool + Send;

pub enum Sdl2Message {
    Lambda(Box<SdlLambda>),
    CreateWindow(Box<SdlCreateWindow>, mpsc::Sender<Option<u32>>),
    HandleEvent(Box<SdlHandleEvent>, mpsc::Sender<()>),
    Exit
}

use Sdl2Message::*;

fn sdl_handler(rx: mpsc::Receiver<Sdl2Message>) {

    // initialization of the library should be the only possible time we panic.
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
                if let Some(canvas) = create_window(&mut sdl_context, &mut video) {
                    let id = canvas.window().id();
                    windows.insert(id, canvas);
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
                // len() should be O(1) according to docs, unlike most linked lists
                let len = unhandled_events.len() as isize;
                let num_events_to_drop = len - 2000;
                for event_num in 0..len {
                    // we're within the length of the list, this unwrap is safe.
                    let event = unhandled_events.pop_front().unwrap();
                    if !handle_event(&mut sdl_context, &mut windows, &event) {
                        // place unhandled events back on the end of the list,
                        // dropping enough of the oldest events to keep at most
                        // 2000 unhandled events, which is enough for several
                        // seconds of collection even during fast user input.
                        //
                        // if no event handler takes responsibility for an event
                        // over the course of several entire seconds, it is then
                        // unlikely to ever be handled by any event handler.
                        if event_num >= num_events_to_drop {
                            unhandled_events.push_back(event);
                        }
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

#[derive(Copy, Clone, Debug)]
pub struct UiThreadExited;

/// map_ute is a 'nop' function that simply converts any type into the UiThreadExited error
#[inline]
fn map_ute<T>(_: T) -> UiThreadExited {
    UiThreadExited
}

impl Sdl2Mt {
    /// A quick, simple way to create a window. Just give it a name, width, and height.
    ///
    /// This function executes synchronously. It will block until the
    /// window_creator function has completed.
    ///
    /// # Panics
    ///
    /// This function will panic if the Window or the Canvas `build()` functions
    /// do not succeed.
    pub fn create_simple_window<IntoString: Into<String>>(&self, name: IntoString, width: u32, height: u32) -> Result<u32, UiThreadExited> {
        let name = name.into();
        self.create_window(Box::new(move |_sdl, video_subsystem| {
            let canvas = video_subsystem.window(&name, width, height)
                .position_centered()
                .resizable()
                .build()
                .unwrap()
                .into_canvas()
                .software()
                .build()
                .unwrap();

            // avoids some potential graphical glitches
            sleep(Duration::from_millis(20));
            
            Some(canvas)
        })).map(|id| id.unwrap())
    }

    /// Executes a window_creator function that accepts &mut VideoSubsystem
    /// and returns an Option<Window>. If Some(window), it will be
    /// added to a HashMap, hashing on the window's ID, which will
    /// then be returned here. If None, None will be returned here.
    ///
    /// This function executes synchronously. It will block until the
    /// window_creator function has completed.
    pub fn create_window(&self, window_creator: Box<SdlCreateWindow>) -> Result<Option<u32>, UiThreadExited> {
        let (tx, rx) = mpsc::channel();
        self.0.send(CreateWindow(window_creator, tx)).map_err(map_ute)?;
        rx.recv().map_err(map_ute)
    }

    //// Executes a lambda function on the UI thread
    //// Either succeeds or the channel is closed and it returns a `SendError`
    ///
    /// This function executes asynchronously. It will *not* block the calling thread.
    pub fn run_on_ui_thread(&self, lambda: Box<SdlLambda>) -> Result<(), UiThreadExited> {
        self.0.send(Lambda(lambda)).map_err(map_ute)
    }

    /// Executes an event_handler function.
    ///
    /// This function executes synchronously. It will block until the
    /// event_handler function has completed.
    pub fn handle_ui_events(&self, event_handler: Box<SdlHandleEvent>) -> Result<(), UiThreadExited> {
        let (tx, rx) = mpsc::channel();
        self.0.send(HandleEvent(event_handler, tx)).map_err(map_ute)?;
        rx.recv().map_err(map_ute)
    }

    /// Terminates the UI thread. Not strictly necessary if the program will exit anyways,
    /// such as when the main program thread returns from main.
    pub fn exit(self) -> Result<(), UiThreadExited> {
        self.0.send(Exit).map_err(map_ute)
    }
}

/// Initializes an `Sdl2Mt` instance, which also initializes the `Sdl2` library.
///
/// # Panics
///
/// `init()` will panic if `Sdl2` initialization fails. If this is unacceptable, you should
/// `catch_panic()` around your `init()` call. Initialization should never fail under
/// anything approaching reasonable circumstances.
pub fn init() -> Sdl2Mt {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || sdl_handler(rx));
    Sdl2Mt(tx)
}