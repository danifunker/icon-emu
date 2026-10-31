//! `icon-cli printout`: render a print job (the bytes the ICON sent its
//! printer, e.g. from `icon-ci printout-raw`) as a PDF, with no machine running.

const USAGE: &str = "icon-cli printout JOB.bin OUT.pdf [--style clean|authentic] [--green-bar] [--worn] [--tractor]

JOB.bin holds the bytes the ICON sent its parallel printer (a Panasonic
KX-P1091 or Epson FX-80). --style clean (default authentic) gives Letter
pages with Courier text; authentic gives 9-pin dots on continuous paper with
the sprocket strips torn off (8.5 x 11, prints 1:1 on Letter). For the
authentic look: --green-bar for green-bar paper, --worn for a worn ribbon,
--tractor to keep the strips (a 9.5-inch sheet with holes and perforations).";

/// Printout options from command-line flags.
pub fn options(args: &[String]) -> Result<icon_printer::Options, String> {
    let style = args.iter().position(|a| a == "--style").and_then(|i| args.get(i + 1)).map_or("authentic", String::as_str);
    let flag = |f: &str| args.iter().any(|a| a == f);
    icon_printer::Options::parse(style, flag("--green-bar"), flag("--worn"), flag("--tractor"))
}

pub fn main(argv: Vec<String>) {
    let files: Vec<&String> = argv.iter().enumerate().filter(|(i, a)| !a.starts_with("--") && (*i == 0 || argv[i - 1] != "--style")).map(|(_, a)| a).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") || files.len() != 2 {
        println!("{USAGE}");
        return;
    }
    let opt = options(&argv).unwrap_or_else(|e| {
        eprintln!("icon-cli printout: {e}");
        std::process::exit(1)
    });
    let job = std::fs::read(files[0]).unwrap_or_else(|e| {
        eprintln!("icon-cli printout: {}: {e}", files[0]);
        std::process::exit(1)
    });
    let doc = icon_printer::interpret(&job);
    std::fs::write(files[1], icon_printer::pdf(&doc, &opt)).unwrap_or_else(|e| {
        eprintln!("icon-cli printout: {}: {e}", files[1]);
        std::process::exit(1)
    });
    println!("{} page(s) written to {}", doc.pages.len(), files[1]);
}
