use point_cloud_viewer::renderer::viewer::RenderThreadBuilderExt;

fn main() {
    // First, we need to choose, which render backend to use.
    // We will select the glium backend here, which is the only available choice at the current
    // state of development.
    // In the future, there could be more backends to choose from, e.g. one that use Vulkan, or
    // Metal on MacOS.
    // Every backend has its own options. We will just go with the default options here.
    let mut options = point_cloud_viewer::renderer::backends::glium::GliumRenderOptions::default();

    // The `run` method from the `RenderThreadBuilderExt` trait starts the render thread with the
    // selected backend.
    // Due to platform compatibility, this **needs** to be called on the main thread. Otherwise,
    // it will panic. Also, it will never return. This means, that sadly you will need to "give up"
    // your main thread. However, you can continue your work in the provided closure.
    options.run(|render_thread| {
        // The closure passed to run() is called as soon as the renderer is fully initialized.
        println!("Initialisation done.");

        // It is passed a handle, that can be used to control the renderer.
        // (opening/closing windows, drawing point clouds, ...)
        // Note, that this does not need to be done from within this closure - the types for
        // controlling the renderer implement both Sync and Send, so they can be transferred to
        // any thread, where they might be needed.

        // The most simple thing to do, is to terminate the render again.
        // Note that it is usually not necessary, to terminate it explicitly,
        // because it is automatically terminated, after this closure returns.
        // However, for the sake of demonstration:
        render_thread.terminate()
    });
}
