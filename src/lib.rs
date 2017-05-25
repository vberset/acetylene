// This file is part of acetylene - Fuel. Efficiently.
//
// acetylene is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// blowtorch is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with blowtorch. If not, see <http://www.gnu.org/licenses/>.

//! Set of tools to write OS images to SD cards and USB sticks.

#[macro_use] extern crate lazy_static;
extern crate regex;
extern crate sha2;

use std::path::Path;
use std::io::{Read,Write};
use std::sync::mpsc::Sender;
use std::fs::{read_dir,metadata,File,OpenOptions};

use sha2::{Sha256,Digest};

use regex::Regex;

const BUFFER4MB: usize = 4 * 1024 * 1024; // 4 MiB

/// Device representation
#[derive(Clone, Debug)]
pub struct Device {
    /// Convenient name
    pub name: String,
    /// Canonical path
    pub path: String,
    /// File size
    pub mbytes: u64,
}

/// Retrieves the canonical path of the specified device's name or path.
pub fn device_path(devices: &Vec<Device>, input: &String) -> Option<String> {
    if input == "/tmp/plop.img" {
        return Some(input.clone());
    }

    for device in devices.iter() {
        if *input == device.name {
            return Some(device.path.clone());
        }
    }

    let path = Path::new(input).canonicalize().unwrap().to_string_lossy().into_owned();

    for device in devices.iter() {
        if path == device.path {
            return Some(path.clone())
        }
    }

    None
}

/// Get the list of available devices.
#[cfg(target_os = "linux")]
pub fn get_device_list() -> Vec<Device> {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            "^(?:mmc|usb)-([^_]*)_[^-]*[^p][^a][^r][^t].?$"
        ).unwrap();
    }

    let mut paths = Vec::new();

    for path in read_dir("/dev/disk/by-id/").unwrap() {
        let path = path.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let path = path.canonicalize().unwrap().to_string_lossy().into_owned();
        let size = metadata(path.clone()).unwrap().len() / (1024*1024);

        if let Some(caps) = RE.captures(&name) {
            paths.push(Device{
                name: (&caps[1]).to_owned(),
                path: path,
                mbytes: size,
            });
        }
    }

    paths
}


/// Get the device size in bytes.
pub fn get_device_size() -> Result<u64,String> {
    Ok(0)
}

#[derive(Clone, Copy, PartialEq)]
pub enum BurnSetting {
    Verify,
}

pub struct BurnConfig {
    /// Destination device
    pub device: String,
    /// Source image
    pub image: String,
    /// Settings
    pub settings: Vec<BurnSetting>,
}

/// Progress events
pub enum Progress {
    Start {
        total: u64,
    },
    Progress {
        count: u64,
        total: u64,
    },
    End {
        digest: Option<Vec<u8>>,
    },
    Error,
}

/// Writes the desired image to the specified device.
pub fn burn_image(config: BurnConfig, tx: Sender<Progress>) {
    let total = metadata(config.image.clone()).unwrap().len();
    let mut image = File::open(config.image).expect("Can't open image");
    let mut device = OpenOptions::new().write(true).open(config.device).expect("Can't open device");

    let verify = config.settings.contains(&BurnSetting::Verify);
    let mut hasher = Sha256::default();

    let mut count = 0;

    let mut buffer = vec![0u8; BUFFER4MB];

    tx.send(Progress::Start{total}).unwrap();

    loop {
        match image.read(&mut *buffer) {
            Ok(0) => {
                let digest = if verify {
                    Some(hasher.result().as_slice().to_owned())
                } else {
                    None
                };

                tx.send(Progress::End {
                    digest: digest
                }).unwrap();

                break;
            }
            Ok(n) => {
                count += n as u64;

                if verify {
                    hasher.input(&buffer[..n]);
                }

                device.write(&buffer[..n]).unwrap();
                device.sync_data().unwrap();

                tx.send(Progress::Progress {
                    count: count,
                    total: total,
                }).unwrap();
            },
            Err(_) => {
                tx.send(Progress::Error).unwrap();

                break;
            }
        }
    }
}
