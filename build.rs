// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if let Some(channel) = version_check::Channel::read() {
        if channel.supports_features() {
            println!("cargo:rustc-cfg=supports_features");
        }
    }
}
