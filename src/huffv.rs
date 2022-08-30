#![doc = include_str!("../README.md")]

use clap::Parser;
use ibig::IBig;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::Formatter;
use std::fs::File;
use std::path::Path;

////////////////////////////////////////////////////////////////
//                         Constants                          //
////////////////////////////////////////////////////////////////

/// The Verifier template contract
pub static HUFF_VERIFIER_CONTRACT: &str = include_str!("contracts/VerifierTemplate.huff");

/// The offset bases for pairing inputs
pub static PI_OFFSET_BASES: [usize; 13] = [
    0x00, 0x20, 0x40, 0x60, 0x80, 0xA0, 0xC0, 0x180, 0x1A0, 0x1C0, 0x240, 0x260, 0x280,
];

////////////////////////////////////////////////////////////////
//                  HUFF SNARK VERIFIER CLI                   //
////////////////////////////////////////////////////////////////

/// Huff SNARK Verifier CLI Args
#[derive(Parser, Debug)]
#[clap(name = "huffv", version, about, long_about = None)]
pub struct HuffVerifier {
    /// The path to the verification key json file generated by snarkjs.
    pub path: Option<String>,

    /// If an output file is designated, the generator will save the verification
    /// contract to a file instead of sending it to stdout.
    #[clap(short = 'o', long = "output")]
    source: Option<String>,
}

/// A SNARK Verification Key.
///
/// Can be directly deserialized from a JSON key generated by
/// [snarkjs](https://github.com/iden3/snarkjs).
#[derive(Serialize, Deserialize, Debug)]
struct VerificationKey {
    #[serde(rename(deserialize = "nPublic", serialize = "nPublic"))]
    pub n_public: u64,

    pub vk_alpha_1: Vec<String>,

    pub vk_beta_2: Vec<Vec<String>>,

    pub vk_gamma_2: Vec<Vec<String>>,

    pub vk_delta_2: Vec<Vec<String>>,

    pub vk_alphabeta_12: Vec<Vec<Vec<String>>>,

    #[serde(rename(deserialize = "IC", serialize = "IC"))]
    pub ic: Vec<Vec<String>>,
}

/// Verification key implementation
impl VerificationKey {
    /// Produce a packed hex representation of the verification key
    pub fn to_packed(&self) -> String {
        // Add alpha, beta, gamma, and delta as the base.
        let mut base = format!(
            "0x{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
            encode_num(&self.vk_alpha_1[0]),
            encode_num(&self.vk_alpha_1[1]),
            encode_num(&self.vk_beta_2[0][1]),
            encode_num(&self.vk_beta_2[0][0]),
            encode_num(&self.vk_beta_2[1][1]),
            encode_num(&self.vk_beta_2[1][0]),
            encode_num(&self.vk_gamma_2[0][1]),
            encode_num(&self.vk_gamma_2[0][0]),
            encode_num(&self.vk_gamma_2[1][1]),
            encode_num(&self.vk_gamma_2[1][0]),
            encode_num(&self.vk_delta_2[0][1]),
            encode_num(&self.vk_delta_2[0][0]),
            encode_num(&self.vk_delta_2[1][1]),
            encode_num(&self.vk_delta_2[1][0]),
        );

        // Push ICs to base verification key
        let n_ics = self.ic.len();
        let mut ics = encode_num(&n_ics.to_string());
        (0..n_ics).for_each(|i| {
            ics.push_str(&encode_num(&self.ic[i][0]));
            ics.push_str(&encode_num(&self.ic[i][1]));
        });
        base.push_str(&ics);

        base
    }
}

impl fmt::Display for VerificationKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string_pretty(self).expect("Failed to serialize verification key.")
        )
    }
}

fn main() {
    let args = HuffVerifier::parse();

    if let Some(path) = args.path {
        let path = Path::new(&path);
        if path.exists() {
            match parse_verification_key(path) {
                Ok(key) => {
                    // Get number of ICs in the verification key
                    let n_ics = key.ic.len();

                    // Fill vkey table with packed verification key
                    let mut contract =
                        HUFF_VERIFIER_CONTRACT.replace("{{PACKED_VKEY}}", &key.to_packed());
                    // Fill n_ics constant
                    contract = contract.replace("{{N_ICS}}", &format!("0x{:02x}", n_ics));
                    // Fill ic_bytes
                    contract = contract.replace("{{IC_BYTES}}", &format!("0x{:02x}", n_ics * 0x40));
                                        
                    // Fill pairing input offsets
                    let pairing_input_offset = 0xC0 + n_ics * 0x40;
                    (0..PI_OFFSET_BASES.len()).for_each(|i| {
                        let tag = format!("{{{{pi_{}}}}}", i);
                        contract = contract.replace(
                            &tag,
                            &format!("0x{:02x}", pairing_input_offset + PI_OFFSET_BASES[i]),
                        );
                    });

                    // Fill public input offsets
                    let input_ptr = pairing_input_offset + 0x300;
                    // Fill pub_input_len_ptr constant
                    contract = contract.replace("{{PUB_INPUT_LEN_PTR}}", &format!("0x{:02x}", input_ptr + 0x100));
                    // Fill pub_input_ptr constant
                    contract = contract.replace("{{PUB_INPUT_PTR}}", &format!("0x{:02x}", input_ptr + 0x120));
                    (0..8).for_each(|i| {
                        let tag = format!("{{{{in_{}}}}}", i);
                        contract = contract.replace(
                            &tag,
                            &format!("0x{:02x}", input_ptr + i * 0x20)
                        );
                    });

                    println!("{}", contract);
                }
                Err(e) => eprintln!("{}", e),
            }
        } else {
            eprintln!("File does not exist!");
        }
    } else {
        eprintln!("No file path provided!");
    }
}

////////////////////////////////////////////////////////////////
//                      Helper Functions                      //
////////////////////////////////////////////////////////////////

/// Parses a verification key from a file path.
fn parse_verification_key(path: &Path) -> Result<VerificationKey, &'static str> {
    if let Ok(contents) = File::open(path) {
        Ok(serde_json::from_reader(contents)
            .expect("Error while deserializing verification key JSON."))
    } else {
        Err("Error reading file contents!")
    }
}

/// Encodes a string that contains a 256 bit decimal number as a 32 byte hex string
fn encode_num(n: &str) -> String {
    let num = IBig::from_str_radix(n, 10).expect("Failed to parse verification key.");
    let mut encoded = num.in_radix(16).to_string();

    // If the encoded hex isn't 32 bytes in length, pad the beginning with
    // zero bytes.
    if encoded.len() != 64 {
        encoded = format!("{}{}", "0".repeat(64 - encoded.len()), encoded);
    }

    encoded
}
