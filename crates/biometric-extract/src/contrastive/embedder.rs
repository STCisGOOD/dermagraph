
use burn::prelude::*;
use burn::nn::{Linear, LinearConfig};
use burn::tensor::activation::relu;

use super::backbone::ResNet18;
use super::center_features::CenterFeatures;

#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    pub embedding_dim: usize,
    pub use_classical: bool,
    pub fusion_hidden_dim: usize,
    pub dropout_rate: f64,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            embedding_dim: 128,
            use_classical: true,
            fusion_hidden_dim: 256,
            dropout_rate: 0.1,
        }
    }
}

#[derive(Module, Debug)]
pub struct FingerprintEmbedder<B: Backend> {
    backbone: ResNet18<B>,

    classical_encoder: Linear<B>,

    fusion: Linear<B>,

    proj1: Linear<B>,

    proj2: Linear<B>,

    embedding_dim: usize,
}

impl<B: Backend> FingerprintEmbedder<B> {
    pub fn new(device: &B::Device, config: EmbedderConfig) -> Self {
        let backbone = ResNet18::new(device);

        let classical_encoder = LinearConfig::new(CenterFeatures::DIM, 64).init(device);

        let fusion_input_dim = ResNet18::<B>::OUTPUT_DIM + 64;
        let fusion = LinearConfig::new(fusion_input_dim, config.fusion_hidden_dim).init(device);

        let proj1 = LinearConfig::new(config.fusion_hidden_dim, config.fusion_hidden_dim).init(device);
        let proj2 = LinearConfig::new(config.fusion_hidden_dim, config.embedding_dim).init(device);

        Self {
            backbone,
            classical_encoder,
            fusion,
            proj1,
            proj2,
            embedding_dim: config.embedding_dim,
        }
    }

    pub fn forward(
        &self,
        images: Tensor<B, 4>,
        classical: Tensor<B, 2>,
    ) -> Tensor<B, 2> {
        let cnn_features = self.backbone.forward(images);

        let classical_encoded = self.classical_encoder.forward(classical);
        let classical_encoded = relu(classical_encoded);

        let fused = Tensor::cat(vec![cnn_features, classical_encoded], 1);

        let fused = self.fusion.forward(fused);
        let fused = relu(fused);

        let proj = self.proj1.forward(fused);
        let proj = relu(proj);
        let embedding = self.proj2.forward(proj);

        l2_normalize(embedding)
    }

    pub fn forward_image_only(&self, images: Tensor<B, 4>) -> Tensor<B, 2> {
        let [batch, _, _, _] = images.dims();
        let device = images.device();

        let classical = Tensor::zeros([batch, CenterFeatures::DIM], &device);

        self.forward(images, classical)
    }

    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }
}

fn l2_normalize<B: Backend>(x: Tensor<B, 2>) -> Tensor<B, 2> {
    let norm = x.clone().powf_scalar(2.0).sum_dim(1).sqrt();
    let norm = norm.clamp_min(1e-8);

    x / norm
}

#[derive(Module, Debug)]
pub struct FingerprintEmbedderLite<B: Backend> {
    backbone: super::backbone::ResNet18Lite<B>,
    projection: Linear<B>,
}

impl<B: Backend> FingerprintEmbedderLite<B> {
    pub fn new(device: &B::Device, embedding_dim: usize) -> Self {
        let backbone = super::backbone::ResNet18Lite::new(device);
        let projection = LinearConfig::new(
            super::backbone::ResNet18Lite::<B>::OUTPUT_DIM,
            embedding_dim,
        ).init(device);

        Self { backbone, projection }
    }

    pub fn forward(&self, images: Tensor<B, 4>) -> Tensor<B, 2> {
        let features = self.backbone.forward(images);
        let embedding = self.projection.forward(features);
        l2_normalize(embedding)
    }
}

pub fn prepare_input<B: Backend>(
    device: &B::Device,
    image_data: &[f32],
    width: usize,
    height: usize,
    classical_features: &CenterFeatures,
) -> (Tensor<B, 4>, Tensor<B, 2>) {
    let image_tensor: Tensor<B, 4> = Tensor::<B, 1>::from_floats(image_data, device)
        .reshape([1, 1, height, width]);

    let classical_vec = classical_features.to_vector();
    let classical_tensor: Tensor<B, 2> = Tensor::<B, 1>::from_floats(classical_vec.as_slice(), device)
        .reshape([1, CenterFeatures::DIM]);

    (image_tensor, classical_tensor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn_ndarray::NdArray;

    type TestBackend = NdArray<f32>;

    #[test]
    fn test_embedder_forward() {
        let device = Default::default();
        let config = EmbedderConfig::default();
        let model = FingerprintEmbedder::<TestBackend>::new(&device, config);

        let images = Tensor::zeros([2, 1, 192, 192], &device);
        let classical = Tensor::zeros([2, CenterFeatures::DIM], &device);

        let embeddings = model.forward(images, classical);

        assert_eq!(embeddings.dims(), [2, 128]);

        let norms = embeddings.clone().powf_scalar(2.0).sum_dim(1).sqrt();
        let norms_data: Vec<f32> = norms.into_data().to_vec().unwrap();
        for norm in norms_data {
            assert!((norm - 1.0).abs() < 1e-5, "Expected norm ~1, got {}", norm);
        }
    }

    #[test]
    fn test_embedder_image_only() {
        let device = Default::default();
        let config = EmbedderConfig::default();
        let model = FingerprintEmbedder::<TestBackend>::new(&device, config);

        let images = Tensor::zeros([1, 1, 192, 192], &device);
        let embeddings = model.forward_image_only(images);

        assert_eq!(embeddings.dims(), [1, 128]);
    }

    #[test]
    fn test_l2_normalize() {
        let device: <TestBackend as Backend>::Device = Default::default();
        let x = Tensor::<TestBackend, 2>::from_floats([[3.0, 4.0], [1.0, 0.0]], &device);
        let normalized = l2_normalize(x);

        let expected = [[0.6, 0.8], [1.0, 0.0]];
        let data: Vec<f32> = normalized.into_data().to_vec().unwrap();

        assert!((data[0] - expected[0][0]).abs() < 1e-5);
        assert!((data[1] - expected[0][1]).abs() < 1e-5);
        assert!((data[2] - expected[1][0]).abs() < 1e-5);
        assert!((data[3] - expected[1][1]).abs() < 1e-5);
    }

    #[test]
    fn test_lite_embedder() {
        let device = Default::default();
        let model = FingerprintEmbedderLite::<TestBackend>::new(&device, 128);

        let images = Tensor::zeros([2, 1, 192, 192], &device);
        let embeddings = model.forward(images);

        assert_eq!(embeddings.dims(), [2, 128]);
    }
}
