extern crate sdl2_mt;

use std::thread::sleep;
use std::time::Duration;

#[test]
#[should_panic]
fn double_exit() {
    let sdlh = sdl2_mt::init();
    let sdlh2 = sdlh.clone();
    
    //do the first exit
    sdlh.exit().unwrap();

    //ensure that the sdl2_mt channel has had time to close
    sleep(Duration::from_millis(200));

    sdlh2.exit().unwrap();
}