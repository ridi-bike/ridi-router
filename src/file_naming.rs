use std::path::{Path, PathBuf};

pub fn route_file_path(
    output_dir: &Path,
    route_index: usize,
    route_length_m: f64,
    extension: &str,
) -> PathBuf {
    output_dir.join(route_file_name(route_index, route_length_m, extension))
}

pub fn route_file_name(route_index: usize, route_length_m: f64, extension: &str) -> String {
    let ordinal = route_index + 1;
    let rounded_km = (route_length_m / 1_000.0).round().max(0.0) as u64;
    let extension = extension.trim_start_matches('.');

    format!("{ordinal:03}-{rounded_km}km.{extension}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{route_file_name, route_file_path};

    #[test]
    fn filename_rounds_distance_to_nearest_whole_kilometer() {
        assert_eq!(route_file_name(0, 12_345.0, "gpx"), "001-12km.gpx");
        assert_eq!(route_file_name(1, 12_500.0, "json"), "002-13km.json");
        assert_eq!(route_file_name(2, 499.0, "json"), "003-0km.json");
    }

    #[test]
    fn filename_uses_expected_extension_for_format() {
        assert_eq!(route_file_name(0, 1_000.0, "gpx"), "001-1km.gpx");
        assert_eq!(route_file_name(0, 1_000.0, ".json"), "001-1km.json");
    }

    #[test]
    fn filename_generation_does_not_imply_stable_ordering() {
        assert_eq!(route_file_name(0, 2_000.0, "json"), "001-2km.json");
        assert_eq!(route_file_name(4, 2_000.0, "json"), "005-2km.json");
    }

    #[test]
    fn route_file_path_joins_output_directory_and_filename() {
        let path = route_file_path(Path::new("/tmp/routes"), 2, 1_234.0, "json");

        assert_eq!(path, Path::new("/tmp/routes/003-1km.json"));
    }
}
