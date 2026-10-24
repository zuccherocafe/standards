# Dynamic image transformations

Add on-the-fly PNG image transformations and disk caching to the static-serving subsystem in `/app`, preserving `Config::from_toml`, `WebServer::new`, and `WebServer::serve(host, path, query)`.

## Requirements

- Support an optional `[images]` configuration table with `enabled` (default `true`), `max_width` and `max_height` (default `4096` each), `default_quality` (`1..=100`, default `85`), `max_file_size` (default `10485760`), and `cache_dir` (default `.cache/images`).
- For PNG files, accept query parameters `w` / `width`, `h` / `height`, `q` / `quality`, `fit` (`contain` default, `cover`, `fill`, `scale-down`), and `format` (`png` default returning `image/png`, `raw_rgba` returning `application/octet-stream`).
- When one dimension is given, compute the other preserving aspect ratio (rounded to nearest, minimum 1); when neither is given, use the source dimensions.
- Refuse invalid configuration or query parameters, duplicate recognised query keys, and sources or target dimensions that exceed limits. Serve original bytes unchanged when no image query key is present or `images.enabled = false`.
- Cache transformed outputs under `cache_dir` so repeated equivalent requests reuse the cached entry without serving stale results when inputs or source files change.
- This is an unattended run with no interactive user replies; use your engineering judgement for any unstated implementation details and implement, test, and commit your complete change on the working branch in `/app`.
