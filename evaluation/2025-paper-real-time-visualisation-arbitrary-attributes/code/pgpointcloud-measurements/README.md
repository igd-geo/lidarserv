# Notizen pgPointCloud
- Docker Container für pgPointCloud Datenbank
    - `docker network create pointcloud_net`
    - `docker run --name pgpointcloud --network=pointcloud_net -p 5432:5432 -e POSTGRES_DB=pointclouds -e POSTGRES_PASSWORD=password -d pgpointcloud/pointcloud`
    - ggf. `docker start pgpointcloud`
    - `docker exec -it pgpointcloud psql -U postgres -d pointclouds -c "\dx"`
    - ip adresse von container: `docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' pgpointcloud`
- Querying Tool ausführen
  - `docker run --rm -it --network=pointcloud_net pgpointcloud/pointcloud psql -h pgpointcloud -U postgres pointclouds`
  - `cargo run --release --bin query -- --input-file input.las`
- PDAL als Input-Conversion-Pipeline
    - Läuft auf Host, nicht in Container ([Installationshinweise](https://pdal.io/en/2.6.0/development/))
    - Installation auf MacOS mit Brew
    - Pipeline konfiguriert über Pipeline.json
        - `readers.LAS` Liest LAS
        - `filters.chipper` Unterteilt Punktwolke in Chunks
            - "While you can theoretically store the contents of a whole file of points in a single patch, it is more practical to store a table full of smaller patches, where the patches are under the PostgreSQL page size (8kb). For most LIDAR data, this practically means a patch size of between 400 and 600 points."
        - `writers.pgpointcloud` Schreibt in pgpointcloud ([Doku](https://pdal.io/en/2.6.0/stages/writers.pgpointcloud.html))
    - Ausführung mit `pdal pipeline --input pipeline.json -v 4` (log level 4)
- Beispiele für Querying:
    - Erste 10 Punkte ausgeben: `SELECT PC_AsText(PC_Explode(pa)) FROM ahn4_15m LIMIT 10;`
    - Filterung innerhalb von Patches: `PC_FilterBetween(pa, 'Classification', 5, 7)`
    - ```rust
        let query_non_existing_class = format!(
            "SELECT SUM(PC_NumPoints(pc_filterequals)) FROM
                (SELECT PC_FilterEquals(pa, 'Classification', {}) FROM {}) AS filtered;",
            19, dataset
        );
        ```
    - Filterung Patches nach Intersection von Dimensionen (räumlich und attribute):
      `SELECT Count(*) FROM ahn4_15m WHERE PC_Intersects(pa, PC_MakePatch(1, ARRAY[-100000]));`