use std::ops::Range;

use anyhow::Context;
use chrono::Utc;
use serde::Deserialize;
use spa::{SolarPos, StdFloatOps};
use zbus::Connection;

use crate::config::{Config, During};

mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::args().nth(1)
        .context("missing config path as first arg")?;
    let config = tokio::fs::read_to_string(&config_path).await
        .context("could not read config")?;
    let config: Config = toml::from_str(&config)
        .context("could not deserialise config")?;

    let now = Utc::now();
    let pos = spa::solar_position::<StdFloatOps>(now, config.location.latitude, config.location.longitude)
        .context("could not determine solar position")?;
    let light = Light::from(pos);

    let wallpaper = config.wallpapers.iter()
        .find(|wp| match &wp.during {
            During::Single(l) if *l == light => true,
            During::Multiple(lights) => lights.iter().any(|&l| l == light),
            _ => false,
        })
        .or_else(|| config.wallpapers.iter().find(|wp| wp.during.is_any()))
        .context("no configured wallpaper")?;

    let connection = Connection::session().await?;
    let proxy = PlasmaShellProxy::new(&connection).await?;
    proxy.evaluate_script(&format!(
        r#"
            var allDesktops = desktops();
            for (i = 0; i < allDesktops.length; i++) {{
                d = allDesktops[i];
                d.wallpaperPlugin = "org.kde.image";
                d.currentConfigGroup = Array(
                    "Wallpaper",
                    "org.kde.image",
                    "General"
                );
                d.writeConfig("Image", "file://{}")
            }}
        "#,
        wallpaper.path.to_string_lossy(),
    )).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
enum Light {
    #[serde(rename = "astronomical dawn")]
    AstronomicalDawn,

    #[serde(rename = "nautical dawn")]
    NauticalDawn,

    #[serde(rename = "civil dawn")]
    CivilDawn,

    #[serde(rename = "day")]
    Day,

    #[serde(rename = "civil dusk")]
    CivilDusk,

    #[serde(rename = "nautical dusk")]
    NauticalDusk,

    #[serde(rename = "astronomical dusk")]
    AstronomicalDusk,

    #[serde(rename = "night")]
    Night,
}

fn to_positive_angle(angle: f64) -> f64 {
    let pos = angle % 360_f64;
    if pos.signum() == -1.0 {
        return 360_f64 + pos;
    }

    pos
}

impl Light {
    const ALL: [Light; 8] = [
        Self::AstronomicalDawn,
        Self::NauticalDawn,
        Self::CivilDawn,
        Self::Day,
        Self::CivilDusk,
        Self::NauticalDusk,
        Self::AstronomicalDusk,
        Self::Night,
    ];

    /// Returns the positive solar altitude bounds of this amount of light.
    fn altitude_bounds(self) -> &'static [Range<f64>] {
        match self {
            Self::AstronomicalDawn => &[(360.0 - 18.0)..(360.0 - 12.0)],
            Self::NauticalDawn => &[(360.0 - 12.0)..(360.0 - 6.0)],
            Self::CivilDawn => &[(360.0 - 6.0)..(360.0 - 0.25)],
            Self::Day => &[
                (360.0 - 0.25)..360.0,
                0.0..(180.0 + 0.25),
            ],
            Self::CivilDusk => &[(180.0 + 0.25)..(180.0 + 6.0)],
            Self::NauticalDusk => &[(180.0 + 6.0)..(180.0 + 12.0)],
            Self::AstronomicalDusk => &[(180.0 + 12.0)..(180.0 + 18.0)],
            Self::Night => &[(180.0 + 18.0)..(360.0 - 18.0)],
        }
    }
}

impl From<SolarPos> for Light {
    fn from(pos: SolarPos) -> Self {
        let altitude = to_positive_angle(90_f64 - pos.zenith_angle);
        for light in Light::ALL {
            if light.altitude_bounds().iter().any(|range| range.contains(&altitude)) {
                return light;
            }
        }

        unreachable!("{altitude}")
    }
}

#[zbus::proxy(
    interface = "org.kde.PlasmaShell",
    default_service = "org.kde.plasmashell",
    default_path = "/PlasmaShell",
)]
trait PlasmaShell {
    #[zbus(name = "evaluateScript")]
    fn evaluate_script(
        &self,
        script: &str,
    ) -> zbus::Result<String>;
}
