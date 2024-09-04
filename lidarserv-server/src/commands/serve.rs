use crate::cli::ServeOptions;
use anyhow::Result;
use lidarserv_server::{
    index::{builder::build, settings::IndexSettings},
    net::server::serve,
};
use log::debug;

#[tokio::main]
pub async fn run(serve_options: ServeOptions) -> Result<()> {
    // load settings
    let settings = IndexSettings::load_from_data_folder(&serve_options.path)?;
    debug!("Loaded settings: {:?}", &settings);

    // init index
    debug!("Building index...");
    let index = build(settings, &serve_options.path)?;

    // handle ctrl+c
    let (shutdown_sender, shutdown_receiver) = tokio::sync::broadcast::channel(1);
    tokio::spawn(async move {
        let mut i = 0;
        loop {
            tokio::signal::ctrl_c().await.unwrap();
            shutdown_sender.send(i).unwrap();
            i += 1;
        }
    });

    // start server
    debug!("Starting server...");
    serve(
        (serve_options.host, serve_options.port),
        index,
        shutdown_receiver,
    )
    .await?;
    Ok(())
}
