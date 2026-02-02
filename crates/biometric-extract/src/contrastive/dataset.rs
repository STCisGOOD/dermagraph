
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintSample {
    pub image_path: PathBuf,

    pub person_id: String,

    pub finger_id: String,

    pub session: u32,

    pub width: u32,

    pub height: u32,
}

pub struct FingerprintDataset {
    samples: Vec<FingerprintSample>,

    person_to_samples: HashMap<String, Vec<usize>>,

    finger_to_samples: HashMap<(String, String), Vec<usize>>,

    person_ids: Vec<String>,
}

impl FingerprintDataset {
    pub fn from_directory<P: AsRef<Path>>(root: P) -> anyhow::Result<Self> {
        let root = root.as_ref();
        let mut samples = Vec::new();

        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let person_id = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            for img_entry in std::fs::read_dir(&path)? {
                let img_entry = img_entry?;
                let img_path = img_entry.path();

                let ext = img_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");

                if !["png", "jpg", "jpeg", "bmp", "tif", "tiff"].contains(&ext.to_lowercase().as_str()) {
                    continue;
                }

                let filename = img_path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");

                let (finger_id, session) = parse_filename(filename);

                let (width, height) = get_image_dimensions(&img_path).unwrap_or((192, 192));

                samples.push(FingerprintSample {
                    image_path: img_path,
                    person_id: person_id.clone(),
                    finger_id,
                    session,
                    width,
                    height,
                });
            }
        }

        Self::from_samples(samples)
    }

    pub fn from_samples(samples: Vec<FingerprintSample>) -> anyhow::Result<Self> {
        let mut person_to_samples: HashMap<String, Vec<usize>> = HashMap::new();
        let mut finger_to_samples: HashMap<(String, String), Vec<usize>> = HashMap::new();

        for (idx, sample) in samples.iter().enumerate() {
            person_to_samples
                .entry(sample.person_id.clone())
                .or_default()
                .push(idx);

            finger_to_samples
                .entry((sample.person_id.clone(), sample.finger_id.clone()))
                .or_default()
                .push(idx);
        }

        let person_ids: Vec<String> = person_to_samples.keys().cloned().collect();

        Ok(Self {
            samples,
            person_to_samples,
            finger_to_samples,
            person_ids,
        })
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn num_persons(&self) -> usize {
        self.person_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<&FingerprintSample> {
        self.samples.get(idx)
    }

    pub fn sample_positive_pair<R: rand::Rng>(&self, rng: &mut R) -> Option<(usize, usize, &str)> {
        let persons_with_multi_fingers: Vec<&String> = self
            .person_to_samples
            .iter()
            .filter(|(_, indices)| indices.len() >= 2)
            .map(|(id, _)| id)
            .collect();

        if persons_with_multi_fingers.is_empty() {
            return None;
        }

        let person_id = persons_with_multi_fingers.choose(rng)?;
        let indices = self.person_to_samples.get(*person_id)?;

        let mut shuffled = indices.clone();
        shuffled.shuffle(rng);

        Some((shuffled[0], shuffled[1], person_id.as_str()))
    }

    pub fn sample_batch<R: rand::Rng>(
        &self,
        batch_size: usize,
        rng: &mut R,
    ) -> Vec<(usize, usize, String)> {
        let mut batch = Vec::with_capacity(batch_size);

        for _ in 0..batch_size {
            if let Some((anchor, positive, person_id)) = self.sample_positive_pair(rng) {
                batch.push((anchor, positive, person_id.to_string()));
            }
        }

        batch
    }

    pub fn get_person_samples(&self, person_id: &str) -> Vec<&FingerprintSample> {
        self.person_to_samples
            .get(person_id)
            .map(|indices| indices.iter().map(|&i| &self.samples[i]).collect())
            .unwrap_or_default()
    }

    pub fn get_finger_samples(
        &self,
        person_id: &str,
        finger_id: &str,
    ) -> Vec<&FingerprintSample> {
        let key = (person_id.to_string(), finger_id.to_string());
        self.finger_to_samples
            .get(&key)
            .map(|indices| indices.iter().map(|&i| &self.samples[i]).collect())
            .unwrap_or_default()
    }

    pub fn train_val_split(&self, val_fraction: f64) -> (Vec<usize>, Vec<usize>) {
        let mut rng = rand::thread_rng();
        let mut person_ids = self.person_ids.clone();
        person_ids.shuffle(&mut rng);

        let val_count = (person_ids.len() as f64 * val_fraction).ceil() as usize;
        let val_persons: std::collections::HashSet<_> =
            person_ids[..val_count].iter().cloned().collect();

        let mut train_indices = Vec::new();
        let mut val_indices = Vec::new();

        for (idx, sample) in self.samples.iter().enumerate() {
            if val_persons.contains(&sample.person_id) {
                val_indices.push(idx);
            } else {
                train_indices.push(idx);
            }
        }

        (train_indices, val_indices)
    }

    pub fn stats(&self) -> DatasetStats {
        let samples_per_person: Vec<usize> = self
            .person_to_samples
            .values()
            .map(|v| v.len())
            .collect();

        let fingers_per_person: Vec<usize> = self
            .person_ids
            .iter()
            .map(|pid| {
                self.finger_to_samples
                    .keys()
                    .filter(|(p, _)| p == pid)
                    .count()
            })
            .collect();

        DatasetStats {
            total_samples: self.samples.len(),
            num_persons: self.person_ids.len(),
            avg_samples_per_person: samples_per_person.iter().sum::<usize>() as f64
                / samples_per_person.len().max(1) as f64,
            avg_fingers_per_person: fingers_per_person.iter().sum::<usize>() as f64
                / fingers_per_person.len().max(1) as f64,
            min_samples_per_person: samples_per_person.iter().copied().min().unwrap_or(0),
            max_samples_per_person: samples_per_person.iter().copied().max().unwrap_or(0),
        }
    }
}

#[derive(Debug)]
pub struct DatasetStats {
    pub total_samples: usize,
    pub num_persons: usize,
    pub avg_samples_per_person: f64,
    pub avg_fingers_per_person: f64,
    pub min_samples_per_person: usize,
    pub max_samples_per_person: usize,
}

fn parse_filename(filename: &str) -> (String, u32) {

    let parts: Vec<&str> = filename.split('_').collect();

    if parts.len() >= 2 {
        if let Ok(session) = parts.last().unwrap().parse::<u32>() {
            let finger = parts[..parts.len() - 1].join("_");
            return (finger, session);
        }
    }

    (filename.to_string(), 1)
}

fn get_image_dimensions(path: &Path) -> Option<(u32, u32)> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);

    let format = image::ImageFormat::from_path(path).ok()?;
    let reader = image::io::Reader::with_format(reader, format);
    let (w, h) = reader.into_dimensions().ok()?;

    Some((w, h))
}

pub fn load_fingerprint_image(
    sample: &FingerprintSample,
    target_size: (u32, u32),
) -> anyhow::Result<Vec<f32>> {
    use image::GenericImageView;

    let img = image::open(&sample.image_path)?;

    let gray = img.to_luma8();

    let resized = image::imageops::resize(
        &gray,
        target_size.0,
        target_size.1,
        image::imageops::FilterType::Lanczos3,
    );

    let pixels: Vec<f32> = resized
        .pixels()
        .map(|p| p.0[0] as f32 / 255.0)
        .collect();

    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filename() {
        assert_eq!(
            parse_filename("thumb_left_001"),
            ("thumb_left".to_string(), 1)
        );
        assert_eq!(
            parse_filename("index_right_023"),
            ("index_right".to_string(), 23)
        );
        assert_eq!(parse_filename("f1_s1"), ("f1_s".to_string(), 1));
    }

    #[test]
    fn test_dataset_from_samples() {
        let samples = vec![
            FingerprintSample {
                image_path: PathBuf::from("p1/thumb_001.png"),
                person_id: "p1".to_string(),
                finger_id: "thumb".to_string(),
                session: 1,
                width: 192,
                height: 192,
            },
            FingerprintSample {
                image_path: PathBuf::from("p1/index_001.png"),
                person_id: "p1".to_string(),
                finger_id: "index".to_string(),
                session: 1,
                width: 192,
                height: 192,
            },
            FingerprintSample {
                image_path: PathBuf::from("p2/thumb_001.png"),
                person_id: "p2".to_string(),
                finger_id: "thumb".to_string(),
                session: 1,
                width: 192,
                height: 192,
            },
        ];

        let dataset = FingerprintDataset::from_samples(samples).unwrap();

        assert_eq!(dataset.len(), 3);
        assert_eq!(dataset.num_persons(), 2);

        assert_eq!(dataset.get_person_samples("p1").len(), 2);
    }
}
