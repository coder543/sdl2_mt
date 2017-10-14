extern crate sdl2_mt;

#[test]
fn open_window() {
    //sdlh is "sdl handle"
    let sdlh = sdl2_mt::init();

    let _window = sdlh.create_window(Box::new(|_sdl, video_subsystem| {
        let window = video_subsystem
            .window("2D plot", 720, 720)
            .position_centered()
            .resizable()
            .build()
            .unwrap()
            .into_canvas()
            .software()
            .build()
            .unwrap();

        Some(window)
    })).unwrap()
        .unwrap();

    sdlh.exit().unwrap();
}
