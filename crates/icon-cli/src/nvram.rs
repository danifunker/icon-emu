//! `icon-cli nvram`: read and edit an ICON NVRAM image (32 bytes, words low
//! byte first, as a chip programmer saves the NMC9306), with no machine running.
//! `icon-ci nvram` does the same on a running network's machines.

use icon_core::nvconfig::{self, Words};

const USAGE: &str = "icon-cli nvram FILE [COMMAND]
  (none) | show                   the settings, as the ICON's nv_config reports them
  set NAME=VALUE ...              change settings and write FILE back
  new fileserver|workstation [NAME=VALUE ...] [--force]
                                  write a fresh image from a built-in one
  hex [W0 ... W15]                print the 16 words, or replace them all
  diff OTHER                      the settings that differ from another image
icon-cli nvram --fields           what NAME can be

FILE is 32 bytes: 16 words, each low byte first (what an EPROM programmer
saves from the ICON's NMC9306 chip). Use it with `icon-cli serve --nvram
SLOT=FILE`, `icon-ci nvram SLOT load FILE`, or the web page's NVRAM panel.";

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("icon-cli nvram: {msg}");
    std::process::exit(1);
}

fn load(path: &str) -> Words {
    let b = std::fs::read(path).unwrap_or_else(|e| fail(format!("{path}: {e}")));
    nvconfig::from_bin(&b).unwrap_or_else(|e| fail(format!("{path}: {e}")))
}

fn save(path: &str, w: &Words) {
    std::fs::write(path, nvconfig::to_bin(w)).unwrap_or_else(|e| fail(format!("{path}: {e}")));
}

pub fn fields_help() -> String {
    nvconfig::FIELDS.iter().map(|(n, h)| format!("  {n:<13} {h}\n")).collect()
}

pub fn main(argv: Vec<String>) {
    match argv.first().map(String::as_str) {
        None | Some("-h" | "--help") => return println!("{USAGE}"),
        Some("--fields") => return print!("{}", fields_help()),
        _ => {}
    }
    let path = &argv[0];
    let rest: Vec<&str> = argv[2.min(argv.len())..].iter().map(String::as_str).collect();
    match argv.get(1).map(String::as_str).unwrap_or("show") {
        "show" => print!("{}", nvconfig::report(&load(path))),
        "set" => {
            if rest.is_empty() {
                fail("set needs NAME=VALUE (icon-cli nvram --fields)");
            }
            let mut w = load(path);
            nvconfig::apply(&mut w, rest).unwrap_or_else(|e| fail(e));
            save(path, &w);
            print!("{}", nvconfig::report(&w));
        }
        "new" => {
            let force = rest.contains(&"--force");
            let mut args = rest.iter().copied().filter(|x| *x != "--force");
            let name = args.next().unwrap_or_else(|| fail("new needs fileserver or workstation"));
            let mut w = nvconfig::preset(name).unwrap_or_else(|| fail(format!("no built-in image {name:?} (fileserver, workstation)")));
            nvconfig::apply(&mut w, args).unwrap_or_else(|e| fail(e));
            if !force && std::path::Path::new(path).exists() {
                fail(format!("{path} exists; add --force to replace it"));
            }
            save(path, &w);
            print!("{}", nvconfig::report(&w));
        }
        "hex" => {
            if rest.is_empty() {
                return println!("{}", nvconfig::to_hex(&load(path)));
            }
            let w = nvconfig::parse_hex(&rest.join(" ")).unwrap_or_else(|e| fail(e));
            save(path, &w);
            print!("{}", nvconfig::report(&w));
        }
        "diff" => {
            let other = rest.first().unwrap_or_else(|| fail("diff needs another image"));
            let (a, b) = (load(path), load(other));
            let (fa, fb) = (nvconfig::fields(&a), nvconfig::fields(&b));
            let mut same = true;
            for (x, y) in fa.iter().zip(&fb) {
                if x.value != y.value {
                    same = false;
                    println!("{:<13} {:<29} {}", x.name, x.value, y.value);
                }
            }
            for i in 0..16 {
                if a[i] != b[i] {
                    same = false;
                    println!("w{i:<12} {:<29} {:04X}", format!("{:04X}", a[i]), b[i]);
                }
            }
            if same {
                println!("same");
            }
        }
        c => fail(format!("unknown command {c:?}\n{USAGE}")),
    }
}
