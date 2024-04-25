use std::{ops::Range, path::PathBuf};

use serde::{de::{value::StrDeserializer, Visitor}, Deserialize};

use crate::Light;

#[derive(Deserialize)]
pub struct Config {
    pub location: Location,
    pub method: Method,
    #[serde(rename = "wallpaper")]
    pub wallpapers: Vec<Wallpaper>,
}

#[derive(Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Deserialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum Method {
    Kde,
}

#[derive(Deserialize)]
pub struct Wallpaper {
    pub during: During,
    pub path: PathBuf,
}

pub enum During {
    Lights(Vec<Light>),
    Elevation {
        rising: Vec<Range<f64>>,
        setting: Vec<Range<f64>>,
    },
    LightsAndElevation {
        lights: Vec<Light>,
        rising: Vec<Range<f64>>,
        setting: Vec<Range<f64>>,
    },
    Any,
}

impl During {
    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }
}

impl<'de> Deserialize<'de> for During {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(DuringVisitor)
    }
}

struct DuringVisitor;

impl<'de> Visitor<'de> for DuringVisitor {
    type Value = During;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string containing 'any', a string contaning a light value, or a sequence containing multiple light values")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut vec: Vec<Light> = Vec::new();
        let mut rising_ranges: Vec<Range<f64>> = Vec::new();
        let mut setting_ranges: Vec<Range<f64>> = Vec::new();

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum F64OrLight {
            F64(f64),
            Light(Light),
        }

        let mut p1 = None;

        while let Some(entry) = seq.next_element::<F64OrLight>()? {
            match entry {
                F64OrLight::Light(light) => vec.push(light),
                F64OrLight::F64(f) => {
                    let mut first = match p1.take() {
                        Some(f) => f,
                        None => {
                            p1 = Some(f % 360.0);
                            continue;
                        }
                    };

                    let mut second = f % 360.0;

                    let first_sign = first.signum();
                    let second_sign = second.signum();
                    if first_sign != second_sign {
                        // invalid
                        continue;
                    }

                    let rising = first_sign == 1.0;
                    let push_to = if rising {
                        &mut rising_ranges
                    } else {
                        &mut setting_ranges
                    };

                    /*
                    [340, 15] # between rising elevations of -20 degrees to 15 degrees
                    [-20..0, 0..15]
                    [-20..15]

                    [-345, -20] # between setting elevations of 15 degrees to -20
                    [0..15, -20..0]
                    [-20..15]
                    */

                    if rising {
                        if first > 180.0 {
                            first -= 360.0;
                        }

                        if second > 180.0 {
                            second -= 360.0;
                        }
                    } else {
                        if first < -180.0 {
                            first += 360.0;
                        }

                        if second < -180.0 {
                            first += 360.0;
                        }
                    }

                    if first > second {
                        if first != 360_f64 {
                            push_to.push(0_f64..first);
                        }

                        if second != 0_f64 {
                            push_to.push(second..0_f64);
                        }
                    } else {
                        push_to.push(first..second);
                    }
                }
            }
        }

        let has_lights = !vec.is_empty();
        let has_elevations = !rising_ranges.is_empty();
        if has_lights && has_elevations {
            Ok(During::LightsAndElevation {
                lights: vec,
                rising: rising_ranges,
                setting: setting_ranges,
            })
        } else if has_lights {
            Ok(During::Lights(vec))
        } else if has_elevations {
            Ok(During::Elevation {
                rising: rising_ranges,
                setting: setting_ranges,
            })
        } else {
            Ok(During::Lights(vec))
        }
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v == "any" {
            return Ok(During::Any);
        }

        let light = Light::deserialize(StrDeserializer::new(v))?;
        Ok(During::Lights(vec![light]))
    }
}
