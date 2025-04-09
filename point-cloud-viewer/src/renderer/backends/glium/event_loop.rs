use crate::renderer::backends::glium::GliumRenderOptions;
use crate::renderer::backends::glium::windows::WindowManager;
use crate::renderer::renderer_command::RendererCommand;
use glium::backend::glutin::glutin::event::Event;
use glium::glutin::event::WindowEvent;
use glium::glutin::event_loop::{ControlFlow, EventLoop as WInitEventLoop, EventLoopBuilder};
use log::{debug, trace};
use std::time::Instant;

pub type EventLoop = WInitEventLoop<RendererCommand>;

pub fn new() -> EventLoop {
    EventLoopBuilder::with_user_event().build()
}

pub fn run(event_loop: EventLoop, options: &GliumRenderOptions) {
    let mut window_manager = WindowManager::new();
    let options = options.clone();

    debug!("Start event loop");
    event_loop.run(move |event, window_target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { window_id, event } => {
                // log
                trace!("Window event: {:?} - {:?}", window_id, event);

                // handle closed windows
                match event {
                    WindowEvent::CloseRequested => {
                        window_manager.close_os(window_id);
                    }
                    WindowEvent::Destroyed => {
                        window_manager.close_os(window_id);
                    }
                    _ => {}
                }

                // handle user input
                if let Some(window) = window_manager.window_by_os_id_mut(window_id) {
                    window.process_window_event(event)
                };
            }
            Event::UserEvent(command) => {
                // renderer commands
                debug!("Command: {:?}", command);
                match command {
                    RendererCommand::Terminate => *control_flow = ControlFlow::Exit,
                    RendererCommand::OpenWindow {
                        closed_notify_sender,
                        response_sender,
                    } => {
                        let result = window_manager.open_window(
                            window_target,
                            closed_notify_sender,
                            options.multisampling,
                        );
                        response_sender.send(result).unwrap();
                    }
                    RendererCommand::CloseWindow { window_id } => window_manager.close(window_id),
                    RendererCommand::UpdateSettings {
                        window_id,
                        new_settings,
                        result_sender,
                    } => {
                        let result = window_manager
                            .window_by_id_mut(window_id)
                            .map(|window| window.update_settings(new_settings))
                            .unwrap_or_else(Err);
                        result_sender.send(result).unwrap();
                    }
                    RendererCommand::UpdateDefaultPointCloudSettings {
                        window_id,
                        new_settings,
                        result_sender,
                    } => {
                        let result = window_manager
                            .window_by_id_mut(window_id)
                            .map(|window| {
                                window.update_default_point_cloud_render_settings(new_settings)
                            })
                            .unwrap_or_else(Err);
                        result_sender.send(result).unwrap();
                    }
                    RendererCommand::UpdatePointCloudSettings {
                        window_id,
                        point_cloud_id,
                        new_settings,
                        result_sender,
                    } => {
                        let result = window_manager
                            .window_by_id_mut(window_id)
                            .map(|window| {
                                window.update_point_cloud_render_settings(
                                    point_cloud_id,
                                    new_settings,
                                )
                            })
                            .unwrap_or_else(Err);
                        result_sender.send(result).unwrap();
                    }
                    RendererCommand::AddPointCloud {
                        window_id,
                        positions,
                        attributes,
                        render_settings,
                        result_sender,
                    } => {
                        let result = window_manager
                            .window_by_id_mut(window_id)
                            .map(|window| {
                                window.add_point_cloud(&positions, &attributes, &render_settings)
                            })
                            .unwrap_or_else(Err);
                        result_sender.send(result).unwrap();
                    }
                    RendererCommand::UpdatePoints {
                        window_id,
                        point_cloud_id,
                        positions,
                        attributes,
                        result_sender,
                    } => {
                        let result = window_manager
                            .window_by_id_mut(window_id)
                            .map(|window| {
                                window.update_points(point_cloud_id, &positions, &attributes)
                            })
                            .unwrap_or_else(Err);
                        result_sender.send(result).unwrap();
                    }
                    RendererCommand::RemovePointCloud {
                        window_id,
                        point_cloud_id,
                        result_sender,
                    } => {
                        let result = window_manager
                            .window_by_id_mut(window_id)
                            .map(|window| window.remove_point_cloud(point_cloud_id))
                            .unwrap_or_else(Err);
                        result_sender.send(result).unwrap();
                    }
                    RendererCommand::CameraMovement {
                        window_id,
                        focus,
                        view,
                        animation,
                        result_sender,
                    } => {
                        let result = window_manager
                            .window_by_id_mut(window_id)
                            .map(|window| window.move_camera(focus, view, animation))
                            .unwrap_or_else(Err);
                        result_sender.send(result).unwrap();
                    }
                    RendererCommand::AddCameraSubscriber {
                        window_id,
                        result_sender,
                    } => {
                        let result = window_manager
                            .window_by_id_mut(window_id)
                            .map(|window| window.add_camera_subscriber());
                        result_sender.send(result).unwrap();
                    }
                }
            }
            Event::RedrawRequested(window_id) => {
                if let Some(window) = window_manager.window_by_os_id_mut(window_id) {
                    let time_start = Instant::now();
                    window.draw().unwrap();
                    let time = Instant::now().duration_since(time_start);
                    trace!("Window draw: {:?} - {} ms", window_id, time.as_millis());
                }
            }
            Event::LoopDestroyed => {
                debug!("Event loop destroyed. This terminates the application.");
            }
            _ => {}
        }
    });
}
