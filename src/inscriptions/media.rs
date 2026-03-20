use {
  super::*,
  mp4::{MediaType, Mp4Reader, TrackType},
  std::{fs::File, io::BufReader},
};

#[derive(Debug, PartialEq, Copy, Clone)]
pub(crate) enum Media {
  Audio,
  Code(Language),
  Font,
  Iframe,
  Image(ImageRendering),
  Markdown,
  Model,
  Pdf,
  Text,
  Unknown,
  Video,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub(crate) enum Language {
  Css,
  JavaScript,
  Json,
  Python,
  Yaml,
}

impl Display for Language {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    write!(
      f,
      "{}",
      match self {
        Self::Css => "css",
        Self::JavaScript => "javascript",
        Self::Json => "json",
        Self::Python => "python",
        Self::Yaml => "yaml",
      }
    )
  }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub(crate) enum ImageRendering {
  Auto,
  Pixelated,
}

impl Display for ImageRendering {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    write!(
      f,
      "{}",
      match self {
        Self::Auto => "auto",
        Self::Pixelated => "pixelated",
      }
    )
  }
}

impl Media {
  const TABLE: &'static [(&'static str, Media, &'static [&'static str])] = &[
    ("application/cbor", Media::Unknown, &["cbor"]),
    ("application/json", Media::Code(Language::Json), &["json"]),
    ("application/octet-stream", Media::Unknown, &["bin"]),
    ("application/pdf", Media::Pdf, &["pdf"]),
    ("application/pgp-signature", Media::Text, &["asc"]),
    ("application/protobuf", Media::Unknown, &["binpb"]),
    ("application/x-bittorrent", Media::Unknown, &["torrent"]),
    (
      "application/x-javascript",
      Media::Code(Language::JavaScript),
      &[],
    ),
    (
      "application/yaml",
      Media::Code(Language::Yaml),
      &["yaml", "yml"],
    ),
    ("audio/flac", Media::Audio, &["flac"]),
    ("audio/mpeg", Media::Audio, &["mp3"]),
    ("audio/ogg", Media::Audio, &[]),
    ("audio/ogg;codecs=opus", Media::Audio, &["opus"]),
    ("audio/wav", Media::Audio, &["wav"]),
    ("font/otf", Media::Font, &["otf"]),
    ("font/ttf", Media::Font, &["ttf"]),
    ("font/woff", Media::Font, &["woff"]),
    ("font/woff2", Media::Font, &["woff2"]),
    (
      "image/apng",
      Media::Image(ImageRendering::Pixelated),
      &["apng"],
    ),
    ("image/avif", Media::Image(ImageRendering::Auto), &[]),
    (
      "image/gif",
      Media::Image(ImageRendering::Pixelated),
      &["gif"],
    ),
    (
      "image/jpeg",
      Media::Image(ImageRendering::Pixelated),
      &["jpg", "jpeg"],
    ),
    ("image/jxl", Media::Image(ImageRendering::Auto), &["jxl"]),
    (
      "image/png",
      Media::Image(ImageRendering::Pixelated),
      &["png"],
    ),
    ("image/svg+xml", Media::Iframe, &["svg"]),
    (
      "image/webp",
      Media::Image(ImageRendering::Pixelated),
      &["webp"],
    ),
    ("model/gltf+json", Media::Model, &["gltf"]),
    ("model/gltf-binary", Media::Model, &["glb"]),
    ("model/stl", Media::Unknown, &["stl"]),
    ("text/css", Media::Code(Language::Css), &["css"]),
    ("text/html", Media::Iframe, &[]),
    ("text/html;charset=utf-8", Media::Iframe, &["html"]),
    (
      "text/javascript",
      Media::Code(Language::JavaScript),
      &["js"],
    ),
    ("text/markdown", Media::Markdown, &[]),
    ("text/markdown;charset=utf-8", Media::Markdown, &["md"]),
    ("text/plain", Media::Text, &[]),
    ("text/plain;charset=utf-8", Media::Text, &["txt"]),
    ("text/x-python", Media::Code(Language::Python), &["py"]),
    ("video/mp4", Media::Video, &["mp4"]),
    ("video/webm", Media::Video, &["webm"]),
  ];

  pub(crate) fn content_type_for_path(path: &Path) -> Result<&'static str, Error> {
    let extension = path
      .extension()
      .ok_or_else(|| anyhow!("file must have extension"))?
      .to_str()
      .ok_or_else(|| anyhow!("unrecognized extension"))?;

    let extension = extension.to_lowercase();

    if extension == "mp4" {
      Media::check_mp4_codec(path)?;
    }

    for (content_type, _, extensions) in Self::TABLE {
      if extensions.contains(&extension.as_str()) {
        return Ok(content_type);
      }
    }

    let mut extensions = Self::TABLE
      .iter()
      .flat_map(|(_, _, extensions)| extensions.first().cloned())
      .collect::<Vec<&str>>();

    extensions.sort();

    Err(anyhow!(
      "unsupported file extension `.{extension}`, supported extensions: {}",
      extensions.join(" "),
    ))
  }

  pub(crate) fn check_mp4_codec(path: &Path) -> Result<(), Error> {
    let f = File::open(path)?;
    let size = f.metadata()?.len();
    let reader = BufReader::new(f);

    let mp4 = Mp4Reader::read_header(reader, size)?;

    for track in mp4.tracks().values() {
      if let TrackType::Video = track.track_type()? {
        let media_type = track.media_type()?;
        if media_type != MediaType::H264 {
          return Err(anyhow!(
            "Unsupported video codec, only H.264 is supported in MP4: {media_type}"
          ));
        }
      }
    }

    Ok(())
  }
}

impl FromStr for Media {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let normalized: String = s.replace("; ", ";");
    for entry in Self::TABLE {
      if entry.0 == normalized {
        return Ok(entry.1);
      }
    }

    // Try matching base type without parameters (e.g. "application/json; charset=utf-8" -> "application/json")
    if let Some(base) = s.split(';').next() {
      let base = base.trim();
      for entry in Self::TABLE {
        if entry.0 == base {
          return Ok(entry.1);
        }
      }
    }

    Err(anyhow!("unknown content type: {s}"))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn for_extension() {
    assert_eq!(
      Media::content_type_for_path(Path::new("pepe.jpg")).unwrap(),
      "image/jpeg"
    );
    assert_eq!(
      Media::content_type_for_path(Path::new("pepe.jpeg")).unwrap(),
      "image/jpeg"
    );
    assert_eq!(
      Media::content_type_for_path(Path::new("pepe.JPG")).unwrap(),
      "image/jpeg"
    );

    assert_regex_match!(
      Media::content_type_for_path(Path::new("pepe.foo")).unwrap_err(),
      r"unsupported file extension `\.foo`, supported extensions: apng asc .*"
    );
  }

  #[test]
  fn content_type_with_extra_parameters() {
    assert_eq!(
      "text/plain; charset=utf-8".parse::<Media>().unwrap(),
      Media::Text
    );
    assert_eq!(
      "text/html; charset=utf-8".parse::<Media>().unwrap(),
      Media::Iframe
    );
    assert_eq!(
      "application/json; charset=utf-8".parse::<Media>().unwrap(),
      Media::Code(Language::Json)
    );
    assert_eq!(
      "text/markdown; charset=utf-8".parse::<Media>().unwrap(),
      Media::Markdown
    );
  }
}
