//! Offline place search for the map picker (D16): reads ExifTool's bundled geolocation
//! database (`lib/Image/ExifTool/Geolocation.dat`, GeoNames cities with population > 2000)
//! directly, so searching by name works without a connection and sends nothing anywhere.
//! Format: see the header comment of ExifTool's Geolocation.pm (database v1.02/1.03).

use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub region: String,
    pub country: String,
    pub country_code: String,
    pub lat: f64,
    pub lon: f64,
    pub population: u64,
}

struct Entry {
    name: String,
    key: String,
    lat: f64,
    lon: f64,
    country: usize,
    region: usize,
    population: u64,
}

pub struct Db {
    entries: Vec<Entry>,
    /// (code, name, folded name)
    countries: Vec<(String, String, String)>,
    /// (name, folded name)
    regions: Vec<(String, String)>,
}

/// Location of `Geolocation.dat` inside an unpacked engine.
pub fn db_path(engine_dir: &Path) -> std::path::PathBuf {
    engine_dir.join("lib").join("Image").join("ExifTool").join("Geolocation.dat")
}

/// Lower-case and strip common accents so "sao paulo" finds "São Paulo".
fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let r = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
            'æ' => "ae",
            'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
            'ď' | 'đ' | 'ð' => "d",
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
            'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
            'ĥ' | 'ħ' => "h",
            'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
            'ĵ' => "j",
            'ķ' => "k",
            'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
            'ñ' | 'ń' | 'ņ' | 'ň' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
            'œ' => "oe",
            'ŕ' | 'ŗ' | 'ř' => "r",
            'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
            'ß' => "ss",
            'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
            'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
            'ŵ' => "w",
            'ý' | 'ÿ' | 'ŷ' => "y",
            'ź' | 'ż' | 'ž' => "z",
            'þ' => "th",
            '-' | '\'' | '’' | '.' => " ",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(r);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Db {
    /// Reads and parses the database file.
    pub fn load(path: &Path) -> Result<Db, String> {
        let data = std::fs::read(path).map_err(|e| format!("The place list couldn't be opened: {e}"))?;
        Self::parse(&data)
    }

    /// Parses the binary format: fixed 13-byte city records, then country and region lists.
    pub fn parse(data: &[u8]) -> Result<Db, String> {
        let bad = || "The place list is in an unexpected format.".to_string();
        let mut pos = 0usize;
        let line = |pos: &mut usize| -> Option<&[u8]> {
            let start = *pos;
            let end = start + data[start..].iter().position(|&b| b == b'\n')?;
            *pos = end + 1;
            Some(&data[start..end])
        };
        let header = String::from_utf8_lossy(line(&mut pos).ok_or_else(bad)?).into_owned();
        let rest = header.strip_prefix("Geolocation").ok_or_else(bad)?;
        let (ver, count) = rest.split_once('\t').ok_or_else(bad)?;
        if ver != "1.02" && ver != "1.03" {
            return Err(bad());
        }
        let count: usize = count.trim().parse().map_err(|_| bad())?;
        line(&mut pos).ok_or_else(bad)?; // comment

        struct Raw {
            lat: f64,
            lon: f64,
            country: usize,
            region: usize,
            population: u64,
            name: String,
        }
        let mut raw = Vec::with_capacity(count);
        for _ in 0..count {
            if pos + 13 > data.len() {
                return Err(bad());
            }
            let b = &data[pos..pos + 13];
            let lt = u16::from_be_bytes([b[0], b[1]]) as u32;
            let f = b[2] as u32;
            let ln = u16::from_be_bytes([b[3], b[4]]) as u32;
            let code = u32::from_be_bytes([b[5], b[6], b[7], b[8]]);
            pos += 13;
            let name_end = pos + data[pos..].iter().position(|&c| c == b'\n').ok_or_else(bad)?;
            let name = String::from_utf8_lossy(&data[pos..name_end]).into_owned();
            pos = name_end + 1;
            let (e, n, fd) = ((code >> 20) & 0xf, (code >> 16) & 0xf, (code >> 12) & 0xf);
            let population = ((n as f64 + fd as f64 / 10.0) * 10f64.powi(e as i32)).round() as u64;
            raw.push(Raw {
                lat: ((lt << 4) | (f >> 4)) as f64 * 180.0 / 1_048_576.0 - 90.0,
                lon: ((ln << 4) | (f & 0x0f)) as f64 * 360.0 / 1_048_576.0 - 180.0,
                country: (code >> 24) as usize,
                region: (code & 0x0fff) as usize,
                population,
                name,
            });
        }
        // Sections that follow are newline-separated lists, each ended by "\0\0\0\0<n>".
        let section = |pos: &mut usize| -> Result<Vec<String>, String> {
            let mut v = vec![];
            loop {
                let l = line(pos).ok_or_else(bad)?;
                if l.len() == 5 && l[..4] == [0, 0, 0, 0] {
                    return Ok(v);
                }
                v.push(String::from_utf8_lossy(l).into_owned());
            }
        };
        let marker = line(&mut pos).ok_or_else(bad)?;
        if !(marker.len() == 5 && marker[..4] == [0, 0, 0, 0]) {
            return Err(bad());
        }
        let countries: Vec<(String, String, String)> = section(&mut pos)?
            .into_iter()
            .map(|c| {
                let (code, name) = if c.len() >= 2 && c.is_char_boundary(2) { (c[..2].to_string(), c[2..].to_string()) } else { (String::new(), c) };
                let folded = fold(&name);
                (code, name, folded)
            })
            .collect();
        let regions: Vec<(String, String)> = section(&mut pos)?.into_iter().map(|r| (fold(&r), r)).map(|(f, r)| (r, f)).collect();
        let entries = raw
            .into_iter()
            .map(|r| Entry { key: fold(&r.name), name: r.name, lat: r.lat, lon: r.lon, country: r.country, region: r.region, population: r.population })
            .collect();
        Ok(Db { entries, countries, regions })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Search by name. "City" or "City, Region/Country" (extra parts narrow the results).
    /// Ranking: exact name, then name starts with the query, then a word starts with it,
    /// then contains it; larger places first within each rank.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Place> {
        let mut parts = query.split(',').map(fold).filter(|s| !s.is_empty());
        let Some(q) = parts.next() else { return vec![] };
        let filters: Vec<String> = parts.collect();
        let mut hits: Vec<(u8, &Entry)> = vec![];
        for e in &self.entries {
            let rank = if e.key == q {
                0
            } else if e.key.starts_with(&q) {
                1
            } else if e.key.split(' ').any(|w| w.starts_with(&q)) {
                2
            } else if q.len() >= 3 && e.key.contains(&q) {
                3
            } else {
                continue;
            };
            if !filters.is_empty() {
                let (cc, cname) = self.countries.get(e.country).map(|c| (c.0.to_lowercase(), c.2.as_str())).unwrap_or_default();
                let rname = self.regions.get(e.region).map(|r| r.1.as_str()).unwrap_or("");
                if !filters.iter().all(|f| cname.contains(f.as_str()) || rname.contains(f.as_str()) || cc == *f) {
                    continue;
                }
            }
            hits.push((rank, e));
        }
        hits.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.population.cmp(&a.1.population)));
        hits.into_iter()
            .take(limit)
            .map(|(_, e)| {
                let (cc, cn) = self.countries.get(e.country).map(|c| (c.0.clone(), c.1.clone())).unwrap_or_default();
                Place {
                    name: e.name.clone(),
                    region: self.regions.get(e.region).map(|r| r.0.clone()).unwrap_or_default(),
                    country: cn,
                    country_code: cc,
                    lat: (e.lat * 10_000.0).round() / 10_000.0,
                    lon: (e.lon * 10_000.0).round() / 10_000.0,
                    population: e.population,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a tiny database in ExifTool's format.
    fn entry(lat: f64, lon: f64, country: u8, region: u16, pop: &str, name: &str) -> Vec<u8> {
        let lt = (((lat + 90.0) / 180.0 * 1_048_576.0) + 0.5) as u32 & 0xfffff;
        let ln = (((lon + 180.0) / 360.0 * 1_048_576.0) + 0.5) as u32 & 0xfffff;
        // pop like "2.1e+06" → N=2 F=1 E=6
        let (n, f, e) = (pop.as_bytes()[0] - b'0', pop.as_bytes()[2] - b'0', pop.as_bytes()[pop.len() - 1] - b'0');
        let code: u32 = ((country as u32) << 24) | ((e as u32) << 20) | ((n as u32) << 16) | ((f as u32) << 12) | region as u32;
        let mut v = vec![];
        v.extend_from_slice(&((lt >> 4) as u16).to_be_bytes());
        v.push((((lt & 0xf) << 4) | (ln & 0xf)) as u8);
        v.extend_from_slice(&((ln >> 4) as u16).to_be_bytes());
        v.extend_from_slice(&code.to_be_bytes());
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(name.as_bytes());
        v.push(b'\n');
        v
    }

    fn sample() -> Db {
        let mut d = b"Geolocation1.03\t4\n# cities with population > 2000\n".to_vec();
        d.extend(entry(48.8534, 2.3488, 0, 0, "2.1e+06", "Paris"));
        d.extend(entry(33.6609, -95.5555, 1, 1, "2.5e+04", "Paris"));
        d.extend(entry(-23.5475, -46.6361, 2, 2, "1.2e+07", "São Paulo"));
        d.extend(entry(45.5088, -73.5878, 3, 3, "1.7e+06", "Montréal"));
        d.extend_from_slice(b"\0\0\0\0\x01\nFRFrance\nUSUnited States\nBRBrazil\nCACanada\n\0\0\0\0\x02\n");
        d.extend_from_slice("Île-de-France\nTexas\nSão Paulo\nQuebec\n\0\0\0\0\x03\n\0\0\0\0\x04\nEurope/Paris\n\0\0\0\0\0\n".as_bytes());
        Db::parse(&d).unwrap()
    }

    #[test]
    fn parses_and_decodes_coordinates() {
        let db = sample();
        assert_eq!(db.len(), 4);
        let r = db.search("paris", 10);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].country, "France", "bigger Paris first");
        assert!((r[0].lat - 48.8534).abs() < 0.001 && (r[0].lon - 2.3488).abs() < 0.001);
        assert_eq!(r[0].population, 2_100_000);
        assert_eq!(r[1].region, "Texas");
    }

    #[test]
    fn accents_prefixes_and_filters() {
        let db = sample();
        assert_eq!(db.search("sao paulo", 5)[0].name, "São Paulo");
        assert_eq!(db.search("montreal", 5)[0].name, "Montréal");
        assert_eq!(db.search("mont", 5)[0].name, "Montréal");
        let r = db.search("Paris, united states", 5);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].country_code, "US");
        assert_eq!(db.search("paris, tx", 5).len(), 0);
        assert_eq!(db.search("paris, texas", 5).len(), 1);
        assert!(db.search("  ", 5).is_empty());
    }
}
