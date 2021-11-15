use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;
use std::thread::sleep;
use std::time::Duration;
use point_cloud_viewer::renderer::settings::{BaseRenderSettings, Color};
use point_cloud_viewer::renderer::error::RendererError;

fn main() {

    point_cloud_viewer::renderer::backends::glium::GliumRenderOptions::default()
        .run(|render_thread| {

            // The `open_window` function gives us a new window that we could show point clouds in.
            let window = render_thread.open_window().unwrap();

            // once we are done, we can close the window again:
            println!("Window will close automatically in 3 seconds...");
            sleep(Duration::from_secs(3));
            window.close();

            // Often, we do not want to close a window right away.
            // Instead, we want to add our point clouds to the window, and then give the user as
            // much time for looking at the points, as he needs.
            // Here, `join` comes in handy. It is a blocking function, that waits until the
            // user closes the window.
            let window = render_thread.open_window().unwrap();
            println!("Close the window to continue");
            window.join();

            // You can open as many windows at the same time, as you want.
            let window_1 = render_thread.open_window().unwrap();
            let window_2 = render_thread.open_window().unwrap();
            println!("Close the windows to continue");
            window_1.join();
            window_2.join();

            // Every window has a bunch of settings,
            // that control basic properties of the window itself (like the window title),
            // as well as general settings for the render (like which background color to use on this window).
            // We can use `set_render_settings`, to customize these settings.
            // Please refer to the documentation for the `BaseRenderSettings` struct for an overview
            // of all customizable settings. Here, we just change the title and make the background
            // color green.
            let window = render_thread.open_window().unwrap();
            window.set_render_settings( BaseRenderSettings {
                window_title: "Hello, World!".to_string(),
                bg_color: Color::GREEN,
                .. Default::default()
            }).unwrap();
            println!("Close the windows to continue");
            window.join();

            // A final note on error handling:
            // The design of the `Window` type generally prevents us from re-using a window that
            // has been closed - both `.close()` and `.join()` consume the window instance.
            // However, this only applies to programmatically closed windows. Also the user could
            // close the window at any time. After the user closed the window, all operations on it
            // will fail with an WindowClosed error, that you might want to handle accordingly, e.g.
            // by terminating the application.
            let window = render_thread.open_window().unwrap();
            println!("Close the windows to exit the application");
            let mut color = Color::rgb(1.0, 0.0, 0.5);
            loop {

                // next background color to set
                color.r += 0.02;
                if color.r > 1.0 {
                    color.r = 0.0;
                }

                // update the background color
                let result = window.set_render_settings(BaseRenderSettings {
                    bg_color: color,
                    .. Default::default()
                });

                // check result and terminate, if user closed the window.
                match result {

                    // exit, if the window was closed
                    Err(RendererError::WindowClosed {..}) => break,

                    // print errors, but continue with the application.
                    Err(e) => {
                        println!("An unexpected error occurred: {}", e)
                    },

                    // Just continue normally on success
                    _ => sleep(Duration::from_secs_f64(0.1)),
                }
            }


        });

}