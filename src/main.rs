use std::{fs, io::BufWriter, path::PathBuf};

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(version, author, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate an entitlements.xml for an app with provisioned entitlements removed
    Strip {
        /// The app to strip entitlements from
        app_path: PathBuf,

        /// File to write the stripped entitlements to
        #[arg(short = 'o', long = "output")]
        output_path: PathBuf,
    },

    /// Generate shell completions
    Completions {
        /// The shell to generate the completions for
        #[arg(value_enum)]
        shell: clap_complete_command::Shell,
    },
}

const PROVISIONED_ENTITLEMENTS: &[&str] = &[
    "com.apple.application-identifier",
    "com.apple.developer.aps-environment",
    "com.apple.developer.associated-domains",
    "com.apple.developer.icloud-container-environment",
    "com.apple.developer.icloud-container-identifiers",
    "com.apple.developer.icloud-services",
    "com.apple.developer.team-identifier",
    "com.apple.developer.ubiquity-container-identifiers",
    "com.apple.developer.ubiquity-kvstore-identifier",
    "com.apple.security.application-groups",
];

fn remove_provisioned_entitlements(entitlements: &mut plist::Value) -> Result<()> {
    let dictionary = entitlements
        .as_dictionary_mut()
        .context("Entitlements is not a dictionary")?;
    for entitlement in PROVISIONED_ENTITLEMENTS {
        dictionary.remove(entitlement);
    }
    Ok(())
}

fn get_entitlements(app_path: &PathBuf) -> Result<plist::Value> {
    let output = std::process::Command::new("/usr/bin/codesign")
        .arg("--display")
        .arg("--xml")
        .arg("--entitlements")
        .arg("-")
        .arg(app_path)
        .output()
        .context("Failed to execute codesign")?;

    if !output.status.success() {
        let stdout =
            String::from_utf8(output.stdout).context("codesign stdout is not valid UTF-8")?;
        let stderr =
            String::from_utf8(output.stderr).context("codesign stderr is not valid UTF-8")?;
        bail!(
            "codesign failed with status {}, stdout: {}, stderr: {}",
            output.status,
            stdout,
            stderr
        );
    }

    let entitlements = plist::from_bytes(&output.stdout)
        .context("Failed to parse entitlements plist from codesign output")?;
    Ok(entitlements)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Strip {
            app_path,
            output_path,
        } => {
            let mut entitlements =
                get_entitlements(&app_path).context("Failed to get entitlements from app")?;
            remove_provisioned_entitlements(&mut entitlements)
                .context("Failed to remove provisioned entitlements")?;

            let writer = fs::File::create(output_path).context("Failed to create output file")?;
            let buf_writer = BufWriter::new(writer);
            plist::to_writer_xml(buf_writer, &entitlements)
                .context("Failed to write stripped entitlements to file")?;
        }
        Commands::Completions { shell } => {
            shell.generate(&mut Cli::command(), &mut std::io::stdout());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remove_provisioned_entitlements_to_string(entitlements_xml: &[u8]) -> String {
        let mut entitlements: plist::Value = plist::from_bytes(entitlements_xml).unwrap();
        remove_provisioned_entitlements(&mut entitlements).unwrap();
        let mut writer = Vec::new();
        let write_options = plist::XmlWriteOptions::default().indent(0, 0);
        plist::to_writer_xml_with_options(&mut writer, &entitlements, &write_options).unwrap();
        String::from_utf8(writer).unwrap()
    }

    #[test]
    fn test_remove_provisioned_entitlements() {
        let entitlements_xml = br#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>com.apple.application-identifier</key><string>AAAAAAAAAA.com.example.example</string><key>com.apple.developer.aps-environment</key><string>production</string><key>com.apple.developer.team-identifier</key><string>AAAAAAAAAA</string><key>com.apple.security.automation.apple-events</key><true/><key>com.apple.security.device.audio-input</key><true/><key>com.apple.security.device.camera</key><true/></dict></plist>"#;
        let stripped_xml =
            remove_provisioned_entitlements_to_string(entitlements_xml).replace('\n', "");
        let expected = r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>com.apple.security.device.camera</key><true/><key>com.apple.security.device.audio-input</key><true/><key>com.apple.security.automation.apple-events</key><true/></dict></plist>"#;
        assert_eq!(stripped_xml, expected);
    }
}
