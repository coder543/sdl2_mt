extern crate sdl2_mt;

use sdl2_mt::event::Event::*;
use sdl2_mt::event::WindowEvent;
use sdl2_mt::keyboard::Keycode;
use sdl2_mt::pixels::Color;

use std::sync::mpsc;
use std::thread::sleep;
use std::time::Duration;

fn main() {
    //sdlh is "sdl handle"
    let sdlh = sdl2_mt::init();

    let window = sdlh.create_simple_window("2D plot", 720, 720).unwrap();

    // example of running arbitrary code on the UI thread
    sdlh.run_on_ui_thread(Box::new(move |_sdl, windows| {
        let canvas = windows.get_mut(&window).unwrap();
        canvas.set_draw_color(Color::RGBA(128, 128, 128, 255));
        canvas.clear();
        canvas.present();
    })).unwrap();

    // create a channel we can use to easily break the loop
    // from inside the closure.
    let (tx, rx) = mpsc::channel();
    while rx.try_recv().is_err() {
        let tx = tx.clone();

        // handle any new UI events that have happened
        sdlh.handle_ui_events(Box::new(move |_sdl, windows, event| {
            match event {
                &Quit { .. } | &KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    // send a message to rx to cancel the loop
                    tx.send(()).unwrap();
                },

                &KeyDown { keycode: Some(keycode), .. } => {
                    use sdl2_mt::video::WindowPos::Positioned;
                    let mut canvas = windows.get_mut(&window).unwrap();
                    let (mut x, mut y) = canvas.window().position();
                    match keycode {
                        Keycode::Up    => y -= 5,
                        Keycode::Down  => y += 5,
                        Keycode::Left  => x -= 5,
                        Keycode::Right => x += 5,
                        _ => {}
                    }
                    canvas.window_mut().set_position(Positioned(x), Positioned(y));
                },

                &Window { win_event: WindowEvent::Resized(new_w, new_h), .. } => {
                    let mut canvas = windows.get_mut(&window).unwrap();
                    canvas.set_draw_color(Color::RGBA(128, (new_h % 256) as u8, (new_w % 256) as u8, 255));
                    canvas.clear();
                    canvas.present();
                },
                
                // false means "this event handler function did not handle this event"
                // in a multithreaded application, you might have an event handler per window.
                // this makes it easier to juggle events between handlers.
                _ => return false
            }
            // true means we handled this event
            true
        })).unwrap();

        // keep the CPU usage down
        sleep(Duration::from_millis(15));
    }

    // not strictly necessary, since when the main thread exits in Rust the entire program is killed.
    // the exit() function has the effect of terminating the SDL2 UI thread.
    sdlh.exit().unwrap();
}