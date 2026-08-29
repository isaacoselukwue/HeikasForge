use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::media::{inspect_gif, inspect_png, MediaExpectation};
use crate::repository::TrackedRepository;

pub const REMOTE_ASSET_RULE: &str = "documentation.no-remote-assets";
pub const MEDIA_RULE: &str = "documentation.media";

pub const REMOTE_ASSET_MARKERS: [&str; 10] = [
    "fonts.googleapis.com",
    "fonts.gstatic.com",
    "cdn.jsdelivr.net",
    "unpkg.com",
    "cdnjs.cloudflare.com",
    "googletagmanager.com",
    "google-analytics.com",
    "plausible.io",
    "hotjar.com",
    "segment.com",
];

pub fn required_media() -> Vec<MediaExpectation> {
    vec![
        MediaExpectation::screenshot("docs/media/dashboard.png"),
        MediaExpectation::screenshot("docs/media/plan-approval.png"),
        MediaExpectation::screenshot("docs/media/run-detail.png"),
        MediaExpectation::screenshot("docs/media/candidate-comparison.png"),
        MediaExpectation::animation("docs/media/demo.gif"),
        MediaExpectation::video("docs/media/demo.webm"),
        MediaExpectation::video("docs/media/demo.mp4"),
    ]
}

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let mut findings = Vec::new();

    for path in &repository.tracked_files {
        let interesting = path.ends_with(".html")
            || path.ends_with(".css")
            || path.ends_with(".ts")
            || path.ends_with(".tsx")
            || path.ends_with(".js");
        if !interesting || !path.starts_with("apps/web/") {
            continue;
        }
        let Some(contents) = repository.read_text(path)? else {
            continue;
        };
        for marker in REMOTE_ASSET_MARKERS {
            if contents.contains(marker) {
                findings.push(
                    PolicyFinding::violation(
                        REMOTE_ASSET_RULE,
                        format!("`{path}` references the remote asset host `{marker}`"),
                        "Bundle the asset locally so the interface makes no third-party request.",
                    )
                    .in_file(path.clone()),
                );
            }
        }
    }

    let Some(readme) = repository.read_text("README.md")? else {
        findings.push(PolicyFinding::violation(
            MEDIA_RULE,
            "the public README is missing",
            "Create README.md with the documented structure.",
        ));
        return Ok(findings);
    };

    for expectation in required_media() {
        let referenced = readme.contains(expectation.path);
        if !referenced {
            findings.push(PolicyFinding::violation(
                MEDIA_RULE,
                format!("the public README does not reference `{}`", expectation.path),
                "Add the media reference to README.md.",
            ));
        }
        let absolute = repository.absolute(expectation.path);
        let Ok(metadata) = std::fs::metadata(&absolute) else {
            findings.push(
                PolicyFinding::violation(
                    MEDIA_RULE,
                    format!("`{}` does not exist", expectation.path),
                    "Capture the media from the running application with the documentation capture task.",
                )
                .in_file(expectation.path.to_string()),
            );
            continue;
        };
        if metadata.len() < expectation.minimum_bytes {
            findings.push(
                PolicyFinding::violation(
                    MEDIA_RULE,
                    format!(
                        "`{}` is {} bytes, which is below the {} byte minimum expected of real captured media",
                        expectation.path,
                        metadata.len(),
                        expectation.minimum_bytes
                    ),
                    "Recapture the media from the running application rather than committing a placeholder.",
                )
                .in_file(expectation.path.to_string()),
            );
            continue;
        }
        if let Some((width, height)) = expectation.expected_dimensions {
            if expectation.path.ends_with(".png") {
                match inspect_png(&absolute) {
                    Ok(Some(actual)) if actual == (width, height) => {}
                    Ok(Some(actual)) => findings.push(
                        PolicyFinding::violation(
                            MEDIA_RULE,
                            format!(
                                "`{}` is {}x{} pixels but {}x{} is required",
                                expectation.path, actual.0, actual.1, width, height
                            ),
                            "Recapture the screenshot at the documented viewport size.",
                        )
                        .in_file(expectation.path.to_string()),
                    ),
                    _ => findings.push(
                        PolicyFinding::violation(
                            MEDIA_RULE,
                            format!("`{}` is not a readable PNG image", expectation.path),
                            "Recapture the screenshot from the running application.",
                        )
                        .in_file(expectation.path.to_string()),
                    ),
                }
            }
            if expectation.path.ends_with(".gif") {
                match inspect_gif(&absolute) {
                    Ok(Some(actual)) if actual.0 > 0 && actual.1 > 0 => {}
                    _ => findings.push(
                        PolicyFinding::violation(
                            MEDIA_RULE,
                            format!("`{}` is not a readable GIF animation", expectation.path),
                            "Regenerate the animation from the captured demonstration frames.",
                        )
                        .in_file(expectation.path.to_string()),
                    ),
                }
            }
        }
    }

    Ok(findings)
}
