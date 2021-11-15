use point_cloud_viewer::renderer::backends::glium::GliumRenderOptions;
use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;
use pasture_core::nalgebra::Point3;
use pasture_core::math::AABB;
use std::thread::sleep;
use std::time::Duration;
use point_cloud_viewer::renderer::settings::{AnimationSettings, AnimationEasing, BaseRenderSettings};

mod utils;

fn main() {
    GliumRenderOptions::default().run(|render_thread| {

        // open window
        let window = render_thread.open_window().unwrap();
        window.set_render_settings(BaseRenderSettings {
            grid: Some(Default::default()),
            .. Default::default()
        }).unwrap();

        // add point clouds
        let point_buffer = utils::small_example_point_cloud(Point3::new(0.0, 0.0, 0.0), 30);
        let point_cloud_id_1 = window.add_point_cloud(&point_buffer).unwrap();
        let point_buffer = utils::small_example_point_cloud(Point3::new(10.0, 10.0, 0.0), 300);
        let point_cloud_id_2 = window.add_point_cloud(&point_buffer).unwrap();
        let point_buffer = utils::attributes_example_point_cloud(Point3::new(5.0, 5.0, 0.0));
        let point_cloud_id_3 = window.add_point_cloud(&point_buffer).unwrap();
        let point_buffer = utils::attributes_example_point_cloud(Point3::new(3.8, 5.0, 0.0));
        let point_cloud_id_4 = window.add_point_cloud(&point_buffer).unwrap();

        // Moving the camera

        // All camera movement is accessible via the `window.camera_movement` function.
        // It returns a builder-style object, that can be used to describe the desired camera movement,
        // and finally execute that movement using camera_movement.execute().

        // The most common way to move the camera will be the `focus_on_*` methods.
        // The concept behind focus is, that we specify, which area we want the user to see.
        // The camera will be automatically placed, such that the full focus area is
        // visible on screen and as big as possible.

        // The most basic method is `focus_on_bounding_box`.
        // It will move the camera, so that everything inside the given bounding box is visible.
        // Here we use it to direct the focus of the user at
        // the second point cloud, the one centered at (10, 10, 0):
        let bounding_box = AABB::from_min_max(
            Point3::new(9.0, 9.0, -0.2),
            Point3::new(11.0, 11.0, 0.2),
        );
        window.camera_movement()
            .focus_on_bounding_box(bounding_box)
            .execute()
            .unwrap();
        sleep(Duration::from_secs(3));

        // However, there is no need to specify the bounding box explicitly.
        // We can use the method `focus_on_point_cloud` instead, to bring a certain
        // point cloud into view.

        // focus on point cloud 1
        window.camera_movement()
            .focus_on_point_cloud(point_cloud_id_1)
            .execute()
            .unwrap();
        sleep(Duration::from_secs(1));

        // focus on point cloud 2
        window.camera_movement()
            .focus_on_point_cloud(point_cloud_id_2)
            .execute()
            .unwrap();
        sleep(Duration::from_secs(1));

        // focus on point cloud 3
        window.camera_movement()
            .focus_on_point_cloud(point_cloud_id_3)
            .execute()
            .unwrap();
        sleep(Duration::from_secs(1));

        // A really convenient method is `focus_on_all`.
        // It makes sure, that all point clouds are fully visible.
        // A common use case is to call this right at the beginning, just after the initial point
        // clouds have been added to the window,
        // so that the user gets a good overview of the full scene.
        window.camera_movement()
            .focus_on_all()
            .execute()
            .unwrap();
        sleep(Duration::from_secs(3));

        // Since this will probably be used at the beginning of most programs, there is also a
        // shorthand function available:
        window.focus_on_all().unwrap();

        // When "jumping around in space", it is easy for the user to get disoriented.
        // A solution to this can be animations. They allow to animate the transition between
        // the old and new camera poses, so that the camera actually "moves" from a to b,
        // rather than being "teleported instantly".
        // We can specify that a camera movement should be animated using the "animated" method.
        // Usually, the default animation settings will produce good results.
        window.camera_movement()
            .focus_on_point_cloud(point_cloud_id_1)
            .animated(Default::default())
            .execute()
            .unwrap();
        sleep(Duration::from_secs_f64(3.75));

        // In some cases, you might want to customize the animation settings.
        // For example, when moving the camera over a very long distance, you might want to
        // make the animation longer so that it does not move too fast.
        // The settings consist of:
        //  - A duration. Shorter durations will make the camera move faster,
        //      longer durations will make it slower. Note, that the animations do not block,
        //      so even if you specify a really long duration, the call to `focus_on_*` will
        //      always return immediately.
        //  - Easing. "Ease in" means, that the camera will not immediately start moving at full
        //      speed, but instead accelerate slowly. Likewise, "Ease out" means, that the camera
        //      will not stop abruptly at the end but smoothly decelerate until it stands still.
        // Here, we demonstrate the four possible easing modes. To see the differences, pay
        // attention to how smoothly the camera movement starts and stops:
        for easing in [
            AnimationEasing::Linear,
            AnimationEasing::EaseIn,
            AnimationEasing::EaseOut,
            AnimationEasing::EaseInOut,
        ] {
            // reset camera
            window.camera_movement()
                .focus_on_point_cloud(point_cloud_id_4)
                .execute()
                .unwrap();
            sleep(Duration::from_secs(2));

            // focus on point cloud 1 with easing
            println!("{:?}", easing);
            let animation = AnimationSettings {
                duration: Duration::from_secs(2),
                easing
            };
            window.camera_movement()
                .focus_on_point_cloud(point_cloud_id_3)
                .animated(animation)
                .execute()
                .unwrap();
            sleep(Duration::from_secs(4));
        }

        // The `view_*` methods allow us to define, from which direction we want the camera to look.
        // When used on its own, the camera will keep looking at the same point, just change its
        // direction from which it is looking.
        // For example, to look down at the point cloud from the top:
        window.camera_movement()
            .view_top()
            .execute()
            .unwrap();
        sleep(Duration::from_secs(3));

        // Of cause, also the `view_*` methods work with animations:
        window.camera_movement()
            .view_right()
            .animated(Default::default())
            .execute()
            .unwrap();
        sleep(Duration::from_secs_f64(1.75));
        window.camera_movement()
            .view_topfront()
            .animated(Default::default())
            .execute()
            .unwrap();
        sleep(Duration::from_secs_f64(1.75));
        window.camera_movement()
            .view_left()
            .animated(Default::default())
            .execute()
            .unwrap();
        sleep(Duration::from_secs_f64(1.75));

        // If we combine a `view_*` with a `focus_on_*`, this will move the camera so that it looks
        // at the specified focus from the indicated direction.

        // Look at all point clouds from the top
        window.camera_movement()
            .view_top()
            .focus_on_all()
            .execute()
            .unwrap();
        sleep(Duration::from_secs(1));

        // animate to look at point_cloud_id_2 from the top left
        window.camera_movement()
            .view_topleft()
            .focus_on_point_cloud(point_cloud_id_2)
            .animated(Default::default())
            .execute()
            .unwrap();

        window.join();
    });
}