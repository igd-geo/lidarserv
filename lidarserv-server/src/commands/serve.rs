use crate::cli::ServeOptions;
use crate::index::builder::build;
use crate::index::settings::IndexSettings;
use crate::net::server::serve;
use anyhow::Result;
use log::debug;

#[tokio::main]
pub async fn run(serve_options: ServeOptions) -> Result<()> {
    // load settings
    let settings = IndexSettings::load_from_data_folder(&serve_options.path)?;
    debug!("Loaded settings: {:?}", &settings);

    // init index
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
    serve(
        (serve_options.host, serve_options.port),
        index,
        shutdown_receiver,
    )
    .await?;
    Ok(())
}
