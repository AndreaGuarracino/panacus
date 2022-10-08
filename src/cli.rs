/* standard crate */
use std::str::FromStr;
/* external crate */
use clap::{Parser, Subcommand};
use regex::Regex;
/* private use */
use crate::graph::*;

#[derive(Parser, Debug)]
#[clap(
    version = "0.2",
    author = "Luca Parmigiani <lparmig@cebitec.uni-bielefeld.de>, Daniel Doerr <daniel.doerr@hhu.de>",
    about = "Calculate count statistics for pangenomic data"
)]

struct Command {
    #[clap(subcommand)]
    cmd: Params,
}

#[derive(Subcommand, Debug)]
pub enum Params {
    #[clap(
        about = "run in default mode, i.e., run hist an growth successively and output only the results of the latter"
    )]
    Growth {
        #[clap(index = 1, help = "graph in GFA1 format", required = true)]
        gfa_file: String,

        #[clap(short, long,
        help = "count type: node or edge count",
        default_value = "nodes",
        possible_values = &["nodes", "edges", "bp"],
    )]
        count: String,

        #[clap(
            name = "subset",
            short,
            long,
            help = "produce counts by subsetting the graph to a given list of paths (1-column list) or path coordinates (3- or 12-column BED file)",
            default_value = ""
        )]
        positive_list: String,

        #[clap(
            name = "exclude",
            short,
            long,
            help = "exclude bps/nodes/edges in growth count that intersect with paths (1-column list) or path coordinates (3- or 12-column BED-file) provided by the given file",
            default_value = ""
        )]
        negative_list: String,

        #[clap(
            short,
            long,
            help = "merge counts from paths by path-group mapping from given tab-separated two-column file",
            default_value = ""
        )]
        groupby: String,

        #[clap(
            short,
            long,
            help = "list of (named) intersection thresholds of the form <level1>,<level2>,.. or <name1>=<level1>,<name2>=<level2> or a file that provides these levels in a tab-separated format; a level is absolute, i.e., corresponds to a number of paths/groups IFF it is integer, otherwise it is a float value representing a percentage of paths/groups.",
            default_value = "cumulative_count=1"
        )]
        intersection: String,

        #[clap(
            short = 'l',
            long,
            help = "list of (named) coverage thresholds of the form <level1>,<level2>,.. or <name1>=<level1>,<name2>=<level2> or a file that provides these levels in a tab-separated format; a level is absolute, i.e., corresponds to a number of paths/groups IFF it is integer, otherwise it is a float value representing a percentage of paths/groups.",
            default_value = "cumulative_count=1"
        )]
        coverage: String,

        #[clap(
            short,
            long,
            help = "run in parallel on N threads",
            default_value = "1"
        )]
        threads: usize,
    },

    #[clap(about = "calculate coverage histogram from GFA file")]
    HistOnly {
        #[clap(index = 1, help = "graph in GFA1 format", required = true)]
        gfa_file: String,

        #[clap(short, long,
        help = "count type: node or edge count",
        default_value = "nodes",
        possible_values = &["nodes", "edges", "bp"],
    )]
        count: String,

        #[clap(
            name = "subset",
            short,
            long,
            help = "produce counts by subsetting the graph to a given list of paths (1-column list) or path coordinates (3- or 12-column BED file)",
            default_value = ""
        )]
        positive_list: String,

        #[clap(
            name = "exclude",
            short,
            long,
            help = "exclude bps/nodes/edges in growth count that intersect with paths (1-column list) or path coordinates (3- or 12-column BED-file) provided by the given file",
            default_value = ""
        )]
        negative_list: String,

        #[clap(
            short,
            long,
            help = "merge counts from paths by path-group mapping from given tab-separated two-column file",
            default_value = ""
        )]
        groupby: String,

        #[clap(
            short,
            long,
            help = "run in parallel on N threads",
            default_value = "1"
        )]
        threads: usize,
    },

    #[clap(about = "construct growth table from coverage histogram")]
    GrowthOnly {
        #[clap(
            index = 1,
            help = "coverage histogram as tab-separated value (tsv) file",
            required = true
        )]
        hist_file: String,

        #[clap(
            short,
            long,
            help = "list of (named) intersection thresholds of the form <level1>,<level2>,.. or <name1>=<level1>,<name2>=<level2> or a file that provides these levels in a tab-separated format; a level is absolute, i.e., corresponds to a number of paths/groups IFF it is integer, otherwise it is a float value representing a percentage of paths/groups.",
            default_value = "cumulative_count=1"
        )]
        intersection: String,

        #[clap(
            short = 'l',
            long,
            help = "list of (named) coverage thresholds of the form <level1>,<level2>,.. or <name1>=<level1>,<name2>=<level2> or a file that provides these levels in a tab-separated format; a level is absolute, i.e., corresponds to a number of paths/groups IFF it is integer, otherwise it is a float value representing a percentage of paths/groups.",
            default_value = "cumulative_count=1"
        )]
        coverage: String,

        #[clap(
            short,
            long,
            help = "run in parallel on N threads",
            default_value = "1"
        )]
        threads: usize,
    },

    #[clap(
        about = "compute growth table for order specified in grouping file (or, if non specified, the order of paths in the GFA file)"
    )]
    OrderedGrowth,
}

pub fn parse_coverage_threshold_cli(threshold_str: &str) -> Vec<(String, Threshold)> {
    let mut coverage_thresholds = Vec::new();

    let re = Regex::new(r"^\s?([!-<,>-~]+)\s?=\s?([!-<,>-~]+)\s*$").unwrap();
    for el in threshold_str.split(',') {
        if let Some(t) = usize::from_str(el.trim()).ok() {
            coverage_thresholds.push((el.trim().to_string(), Threshold::Absolute(t)));
        } else if let Some(t) = f64::from_str(el.trim()).ok() {
            coverage_thresholds.push((el.trim().to_string(), Threshold::Relative(t)));
        } else if let Some(caps) = re.captures(&el) {
            let name = caps.get(1).unwrap().as_str().trim().to_string();
            let threshold_str = caps.get(2).unwrap().as_str();
            let threshold = if let Some(t) = usize::from_str(threshold_str).ok() {
                Threshold::Absolute(t)
            } else {
                Threshold::Relative(f64::from_str(threshold_str).unwrap())
            };
            coverage_thresholds.push((name, threshold));
        } else {
            panic!(
                "coverage threshold \"{}\" string is not well-formed",
                &threshold_str
            );
        }
    }

    coverage_thresholds
}

pub fn read_params() -> Params {
    Command::parse().cmd
}
