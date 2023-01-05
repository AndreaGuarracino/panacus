/* standard use */
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::iter::FromIterator;
use std::str::{self, FromStr};
/* external crate */
use quick_csv::Csv;
use rayon::prelude::*;
//use std::sync::{Arc, Mutex};
/* private use */
use crate::abacus::*;
use crate::graph::*;
use crate::util::*;

pub fn parse_bed<R: Read>(data: &mut BufReader<R>) -> Vec<PathSegment> {
    let mut res = Vec::new();

    let reader = Csv::from_reader(data)
        .delimiter(b'\t')
        .flexible(true)
        .has_header(false);
    let mut is_header = true;
    let mut is_full_bed = false;
    for (i, row) in reader.enumerate() {
        let row = row.unwrap();
        let mut row_it = row.bytes_columns();
        let path_name = str::from_utf8(row_it.next().unwrap()).unwrap().to_string();
        // recognize BED header
        if is_header
            && (path_name.starts_with("browser ")
                || path_name.starts_with("track ")
                || path_name.starts_with("#"))
        {
            continue;
        }
        is_header = false;
        let mut path_seg = PathSegment::from_str(&path_name);
        if let Some(start) = row_it.next() {
            if let Some(end) = row_it.next() {
                path_seg.start = usize::from_str(str::from_utf8(start).unwrap()).ok();
                path_seg.end = usize::from_str(str::from_utf8(end).unwrap()).ok();
            } else {
                panic!(
                    "erroneous input in line {}: row must have either 1, 3, or 12 columns, but has 2",
                    i
                );
            }
            if let Some(block_count_raw) = row_it.nth(6) {
                if !is_full_bed {
                    log::debug!("assuming from now (line {}) on that file is in full bed (12 columns) format", i);
                }
                let block_count =
                    usize::from_str(str::from_utf8(block_count_raw).unwrap()).unwrap();
                is_full_bed = true;
                let mut block_sizes = str::from_utf8(row_it.next().unwrap()).unwrap().split(',');
                let mut block_starts = str::from_utf8(row_it.next().unwrap()).unwrap().split(',');
                for _ in 0..block_count {
                    let size = usize::from_str(block_sizes.next().unwrap().trim()).unwrap();
                    let start = usize::from_str(block_starts.next().unwrap().trim()).unwrap();

                    let mut tmp = path_seg.clone();
                    if tmp.start.is_some() {
                        tmp.start = Some(tmp.start.unwrap() + start);
                    } else {
                        tmp.start = Some(start);
                    }
                    tmp.end = Some(start + size);
                    res.push(tmp);
                }
            }
        }
        if !is_full_bed {
            res.push(path_seg);
        }
    }

    res
}

pub fn parse_groups<R: Read>(
    data: &mut BufReader<R>,
) -> Result<Vec<(PathSegment, String)>, std::io::Error> {
    let mut res: Vec<(PathSegment, String)> = Vec::new();

    let mut visited: HashSet<PathSegment> = HashSet::default();
    let reader = Csv::from_reader(data)
        .delimiter(b'\t')
        .flexible(true)
        .has_header(false);
    for (i, row) in reader.enumerate() {
        let row = row.unwrap();
        let mut row_it = row.bytes_columns();
        let path_seg =
            PathSegment::from_str(&str::from_utf8(row_it.next().unwrap()).unwrap().to_string());
        if visited.contains(&path_seg) {
            let msg = format!(
                "error in line {}: path segment {} has been already assigned to a group",
                i, &path_seg
            );
            log::error!("{}", &msg);
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, msg));
        }
        visited.insert(path_seg.clone());
        //        if path_seg.coords().is_some() {
        //            let msg = format!("error in line {}: coordinates are not permitted in grouping paths",
        //                i);
        //            log::error!("{}", &msg);
        //            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,  msg));
        //        }
        res.push((
            path_seg,
            str::from_utf8(row_it.next().unwrap()).unwrap().to_string(),
        ));
    }

    Ok(res)
}

pub fn parse_coverage_threshold_file<R: Read>(data: &mut BufReader<R>) -> Vec<(String, Threshold)> {
    let mut res = Vec::new();

    let reader = Csv::from_reader(data)
        .delimiter(b'\t')
        .flexible(true)
        .has_header(false);
    for row in reader {
        let row = row.unwrap();
        let mut row_it = row.bytes_columns();
        let name = str::from_utf8(row_it.next().unwrap())
            .unwrap()
            .trim()
            .to_string();
        let threshold = if let Some(col) = row_it.next() {
            let threshold_str = str::from_utf8(col).unwrap();
            if let Some(t) = usize::from_str(threshold_str).ok() {
                Threshold::Absolute(t)
            } else {
                Threshold::Relative(f64::from_str(threshold_str).unwrap())
            }
        } else {
            if let Some(t) = usize::from_str(&name[..]).ok() {
                Threshold::Absolute(t)
            } else {
                Threshold::Relative(f64::from_str(&name[..]).unwrap())
            }
        };
        res.push((name, threshold));
    }

    res
}

pub fn parse_walk_identifier<'a>(data: &'a [u8]) -> (PathSegment, &'a [u8]) {
    let mut six_col: Vec<&str> = Vec::with_capacity(6);

    let mut it = data.iter();
    let mut i = 0;
    for _ in 0..6 {
        let j = it.position(|x| x == &b'\t').unwrap();
        six_col.push(&str::from_utf8(&data[i..i + j]).unwrap());
        i += j + 1;
    }

    let seq_start = match six_col[4] {
        "*" => None,
        a => Some(usize::from_str(a).unwrap()),
    };

    let seq_end = match six_col[5] {
        "*" => None,
        a => Some(usize::from_str(a).unwrap()),
    };

    let path_seg = PathSegment::new(
        six_col[1].to_string(),
        six_col[2].to_string(),
        six_col[3].to_string(),
        seq_start,
        seq_end,
    );

    (path_seg, &data[i..])
}

fn parse_walk_seq(
    data: &[u8],
    node2id: &HashMap<Vec<u8>, u32>,
    node_len: &Vec<u32>,
    offset: usize,
    subset_coords: &[(usize, usize)],
    node_table: &mut ItemTable,
    num_walk: usize,
) {
    let mut it = data.iter();
    let end = it
        .position(|x| x == &b'\t' || x == &b'\n' || x == &b'\r')
        .unwrap();

    log::debug!("parsing path sequences of size {}..", end);

    // XXX ignore first > | < so that no empty is created for 1st node
    let sids: Vec<u32> = data[1..end]
        .par_split(|&x| x == b'<' || x == b'>')
        .map(|node| {
            *node2id.get(&node[..]).expect(
                &format!(
                    "walk contains unknown node {} ",
                    str::from_utf8(&node[..]).unwrap()
                )[..],
            )
        })
        .collect();

    let mut i = 0;
    let mut p = offset;

    for sid in sids {
        // update current pointer in subset_coords list
        while i < subset_coords.len() && subset_coords[i].1 <= p {
            i += 1;
        }

        let l = node_len[sid as usize] as usize;

        // check if the current position fits within active segment
        if i < subset_coords.len() && subset_coords[i].0 <= p + l {
            let idx = (sid as usize) % SIZE_T;
            node_table.items[idx].push(sid);
            node_table.id_prefsum[idx][num_walk + 1] += 1;
        }
        p += l;
    }

    // compute prefix sum
    for i in 0..SIZE_T {
        node_table.id_prefsum[i][num_walk + 1] += node_table.id_prefsum[i][num_walk];
    }
}

pub fn parse_path_identifier<'a>(data: &'a [u8]) -> (PathSegment, &'a [u8]) {
    let mut iter = data.iter();

    let start = iter.position(|&x| x == b'\t').unwrap() + 1;
    let offset = iter.position(|&x| x == b'\t').unwrap();
    let path_name = str::from_utf8(&data[start..start + offset]).unwrap();

    (
        PathSegment::from_str(path_name),
        &data[start + offset + 1..],
    )
}

pub fn parse_graph_marginals<R: Read>(
    data: &mut BufReader<R>,
    index_edges: bool,
) -> (
    HashMap<Vec<u8>, u32>,
    Vec<u32>,
    Option<HashMap<Vec<u8>, u32>>,
    Vec<PathSegment>,
) {
    let mut node_count = 0;
    let mut edge_count = 0;
    let mut node2id: HashMap<Vec<u8>, u32> = HashMap::default();
    let mut edge2id: Option<HashMap<Vec<u8>, u32>> = if index_edges {
        Some(HashMap::default())
    } else {
        None
    };
    let mut path_segments: Vec<PathSegment> = Vec::new();
    let mut node_len: Vec<u32> = Vec::new();

    let mut buf = vec![];
    while data.read_until(b'\n', &mut buf).unwrap_or(0) > 0 {
        if buf[0] == b'S' {
            let mut iter = buf.iter();
            let start = iter.position(|&x| x == b'\t').unwrap() + 1;
            let offset = iter.position(|&x| x == b'\t').unwrap();
            let sid = buf[start..start + offset].to_vec();
            let offset = iter
                .position(|&x| x == b'\t' || x == b'\n' || x == b'\r')
                .unwrap();
            node_len.push(offset as u32);
            node2id.entry(sid).or_insert(node_count);
            node_count += 1;
        } else if index_edges && buf[0] == b'L' {
            let mut iter = buf.iter();
            let start = iter.position(|&x| x == b'\t').unwrap() + 1;
            let offset = iter.position(|&x| x == b'\t').unwrap();
            let sid1 = buf[start..start + offset].to_vec();

            // we know that 3rd colum is either '+' or '-', so it has always length 1; still, we
            // need to advance in the buffer (and  therefore call iter.position(..))
            iter.position(|&x| x == b'\t');
            let o1 = if buf[offset + 1] == b'+' { b'>' } else { b'<' };

            let start = start + 2;
            let offset = iter.position(|&x| x == b'\t').unwrap();
            let sid2 = buf[start..start + offset].to_vec();

            let o2 = if buf[offset + 1] == b'+' { b'>' } else { b'<' };

            let lid: Vec<u8> = vec![o1]
                .into_iter()
                .chain(sid1.into_iter())
                .chain(vec![o2].into_iter())
                .chain(sid2.into_iter())
                .collect();
            edge2id.as_mut().unwrap().entry(lid).or_insert(edge_count);
            edge_count += 1;
        } else if buf[0] == b'P' {
            let (path_seg, _) = parse_path_identifier(&buf);
            path_segments.push(path_seg);
        } else if buf[0] == b'W' {
            let (path_seg, _) = parse_walk_identifier(&buf);
            path_segments.push(path_seg);
        }

        buf.clear();
    }

    (node2id, node_len, edge2id, path_segments)
}

fn parse_path_seq<F, G>(
    data: &[u8],
    graph_marginals: &GraphData,
    offset: usize,
    subset_coords: &[(usize, usize)],
    exclude_coords: &[(usize, usize)],
    mut match_f: F,
    mut exlude_f: G,
) where
    // node id (sid), uncovered basepairs (nonzero only if subset coords start within the node)
    F: FnMut(u32, usize),
    // node id (sid), uncovered basepairs (nonzero only if exclusion coords start within the node)
    G: FnMut(u32, usize),
{
    // uncomment code for dynamic table construction in "1-pass" reading of GFA file when the total
    // number of paths is unknown
    //
    //    // ensure there's enough space in the ItemTable
    //    for i in 0..SIZE_T {
    //        let l = node_table.id_prefsum[i].len();
    //        if l < num_path + 1 {
    //            node_table.id_prefsum[i].extend(vec![0; num_path + 1 - l]);
    //        }
    //    }

    let mut it = data.iter();
    let end = it
        .position(|x| x == &b'\t' || x == &b'\n' || x == &b'\r')
        .unwrap();

    log::debug!("parsing path sequences of size {}..", end);

    let sids: Vec<u32> = data[..end]
        .par_split(|&x| x == b',')
        .map(|node| {
            // Parallel
            //path_data.split(|&x| x == b',').for_each( |node| {  // Sequential
            let sid = *graph_marginals
                .node2id
                .get(&node[0..node.len() - 1])
                .expect(&format!(
                    "unknown node {}",
                    &str::from_utf8(node).unwrap()[..]
                ));
            let o = node[node.len() - 1];
            assert!(
                o == b'-' || o == b'+',
                "unknown orientation of segment {}",
                str::from_utf8(&node).unwrap()
            );

            sid
        })
        .collect();

    let mut i = 0;
    let mut p = offset;

    for sid in sids {
        // update current pointer in subset_coords list
        while i < subset_coords.len() && subset_coords[i].1 <= p {
            i += 1;
        }

        let l = graph_marginals.node_len[sid as usize] as usize;

        // check if the current position fits within active segment
        if i < subset_coords.len() && subset_coords[i].0 <= p + l {
            match_f(
                sid,
                if subset_coords[i].0 > p {
                    subset_coords[i].0 - p
                } else {
                    0
                } + if subset_coords[i].1 < p + l {
                    subset_coords[i].1 - p - l
                } else {
                    0
                },
            );
        } else if i >= subset_coords.len() {
            // terminate parse if all subset coords are processed
            break;
        }
        p += l;
    }

    log::debug!("..done");
}

fn build_subpath_map(subset_coords: &Vec<PathSegment>) -> HashMap<String, Vec<(usize, usize)>> {
    let mut res: HashMap<String, HashSet<(usize, usize)>> = HashMap::default();

    subset_coords.into_iter().for_each(|x| {
        res.entry(x.id())
            .or_insert(HashSet::default())
            .insert(match x.coords() {
                None => (0, usize::MAX),
                Some((i, j)) => (i, j),
            });
    });

    HashMap::from_iter(res.into_iter().map(|(pid, coords)| {
        let mut v: Vec<(usize, usize)> = coords.into_iter().collect();
        v.sort();
        (pid, v)
    }))
}

pub fn parse_gfa_nodecount<R: Read>(
    data: &mut BufReader<R>,
    abacus_data: &AbacusData,
    graph_marginals: &GraphData,
) -> ItemTable {
    let mut node_table = ItemTable::new(graph_marginals.path_segments.len());

    let subset_map = match &abacus_data.subset_coords {
        None => HashMap::default(),
        Some(coords) => build_subpath_map(coords),
    };

    // Reading GFA file searching for (P)aths and (W)alks
    let mut buf = vec![];
    let mut num_path = 0;
    let complete: Vec<(usize, usize)> = vec![(0, usize::MAX)];
    while data.read_until(b'\n', &mut buf).unwrap_or(0) > 0 {
        if buf[0] == b'P' {
            let (path_seg, buf_path_seg) = parse_path_identifier(&buf);
            log::debug!("updating count data structure..");
            parse_path_seq(
                &buf_path_seg,
                &graph_marginals,
                path_seg.coords().get_or_insert((0, 0)).0,
                if abacus_data.subset_coords.is_none() {
                    &complete[..]
                } else {
                    match subset_map.get(&path_seg.id()) {
                        // empty slice
                        None => &complete[1..],
                        Some(coords) => &coords[..],
                    }
                },
                &[],
                |sid, uncovered| {
                    // only count nodes that are completely contained in subset coords
                    if uncovered == 0 {
                        let idx = (sid as usize) % SIZE_T;
                        node_table.items[idx].push(sid);
                        node_table.id_prefsum[idx][num_path + 1] += 1;
                    }
                },
                |_, _1| {},
            );
            // Compute prefix sum
            for i in 0..SIZE_T {
                node_table.id_prefsum[i][num_path + 1] += node_table.id_prefsum[i][num_path];
            }
            num_path += 1;
            log::debug!("done");
        } else if buf[0] == b'W' {
            let (path_seg, buf_walk_seq) = parse_walk_identifier(&buf);
            log::debug!("updating count data structure..");
            parse_walk_seq(
                &buf_walk_seq,
                &graph_marginals.node2id,
                &graph_marginals.node_len,
                path_seg.coords().get_or_insert((0, 0)).0,
                if abacus_data.subset_coords.is_none() {
                    &complete[..]
                } else {
                    match subset_map.get(&path_seg.id()) {
                        // empty slice
                        None => &complete[1..],
                        Some(coords) => &coords[..],
                    }
                },
                &mut node_table,
                num_path,
            );
            log::debug!("done");
            num_path += 1;
        }
        buf.clear();
    }
    node_table
}
