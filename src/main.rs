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
    let elevation = 90_f64 - pos.zenith_angle;
    let light = Light::from(pos.clone());
    // let declination = calculate_declination(now);
    // // solar noon is when zenith angle is equal to latitude - solar declination angle
    // println!("light: {light:?}");
    // println!("declination: {declination:?}");
    // println!("azimuth: {}", pos.azimuth);
    // println!("zenith angle: {}", pos.zenith_angle);
    // println!("altitude: {altitude}");
    // println!("solar noon: {}", config.location.latitude - declination.unwrap_or_default());

    let wallpaper = config.wallpapers.iter()
        .find(|wp| match &wp.during {
            During::Lights(lights) => lights.iter().any(|&l| l == light),
            During::Elevation { rising, setting } => (pos.azimuth < 180.0 && rising.iter().any(|range| range.contains(&elevation)))
                || (pos.azimuth > 180.0 && setting.iter().any(|range| range.contains(&elevation))),
            During::LightsAndElevation { lights, rising, setting } => {
                lights.iter().any(|&l| l == light)
                || (pos.azimuth < 180.0 && rising.iter().any(|range| range.contains(&elevation)))
                || (pos.azimuth > 180.0 && setting.iter().any(|range| range.contains(&elevation)))
            }
            During::Any => false,
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
                d.writeConfig("Image", "file://{}");
            }}
        "#,
        wallpaper.path.to_string_lossy(),
    )).await?;
    Ok(())
}

// /// Calculates the Sun's declination in degrees.
// fn calculate_declination(date: DateTime<Utc>) -> Option<f64> {
//     // -arcsin{0.39779cos[0.98565°(N + 10) + 1.914°sin(0.98565°(N - 2))]}
//     let first_jan = Utc.with_ymd_and_hms(date.year(), 1, 1, 0, 0, 0).earliest()?;
//     let n = date.signed_duration_since(first_jan).num_days() as f64;
//     let a = (1.914_f64).to_radians();
//     let b = (0.98565_f64).to_radians();

//     let left = b * (n + 10_f64);
//     let right = a * (b * (n - 2_f64)).sin();
//     let cos = (left + right).cos();
//     let result = -(0.39779_f64 * cos).asin();

//     Some(result.to_degrees())
// }

#[derive(Debug, Clone, Copy)]
enum LightGeneric {
    Day,
    Night,
    AstronomicalTwilight,
    NauticalTwilight,
    CivilTwilight,
}

impl LightGeneric {
    const ALL: [LightGeneric; 5] = [
        Self::Day,
        Self::Night,
        Self::AstronomicalTwilight,
        Self::NauticalTwilight,
        Self::CivilTwilight,
    ];

    fn altitude_bounds(self) -> &'static [Range<f64>] {
        match self {
            Self::Day => &[
                -0.25..0.0,
                0.0..360.00,
            ],
            Self::AstronomicalTwilight => &[-18.0..-12.0],
            Self::NauticalTwilight => &[-12.0..-6.0],
            Self::CivilTwilight => &[-6.0..-0.25],
            Self::Night => &[-360.0..-18.0],
        }
    }

    fn to_specific(self, azimuth: f64) -> Light {
        match self {
            Self::Day => Light::Day,
            Self::Night => Light::Night,
            Self::AstronomicalTwilight => if azimuth < 180.0 {
                Light::AstronomicalDawn
            } else {
                Light::AstronomicalDusk
            },
            Self::NauticalTwilight => if azimuth < 180.0 {
                Light::NauticalDawn
            } else {
                Light::NauticalDusk
            },
            Self::CivilTwilight => if azimuth < 180.0 {
                Light::CivilDawn
            } else {
                Light::CivilDusk
            },
        }
    }
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

impl From<SolarPos> for Light {
    fn from(pos: SolarPos) -> Self {
        let elevation = 90_f64 - pos.zenith_angle;
        for light in LightGeneric::ALL {
            if light.altitude_bounds().iter().any(|range| range.contains(&elevation)) {
                return light.to_specific(pos.azimuth);
            }
        }

        unreachable!("{elevation}")
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
