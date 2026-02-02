
use burn::prelude::*;
use burn::nn::{
    conv::{Conv2d, Conv2dConfig},
    pool::{AdaptiveAvgPool2d, AdaptiveAvgPool2dConfig},
    BatchNorm, BatchNormConfig, PaddingConfig2d,
};
use burn::tensor::activation::relu;

#[derive(Module, Debug)]
pub struct DownsampleLayer<B: Backend> {
    conv: Conv2d<B>,
    bn: BatchNorm<B>,
}

impl<B: Backend> DownsampleLayer<B> {
    pub fn new(device: &B::Device, in_channels: usize, out_channels: usize, stride: usize) -> Self {
        let conv = Conv2dConfig::new([in_channels, out_channels], [1, 1])
            .with_stride([stride, stride])
            .init(device);
        let bn = BatchNormConfig::new(out_channels).init(device);
        Self { conv, bn }
    }

    pub fn forward(&self, x: Tensor<B, 4>) -> Tensor<B, 4> {
        let x = self.conv.forward(x);
        self.bn.forward(x)
    }
}

#[derive(Module, Debug)]
pub struct BasicBlock<B: Backend> {
    conv1: Conv2d<B>,
    bn1: BatchNorm<B>,
    conv2: Conv2d<B>,
    bn2: BatchNorm<B>,
    downsample: Option<DownsampleLayer<B>>,
}

impl<B: Backend> BasicBlock<B> {
    pub fn new(
        device: &B::Device,
        in_channels: usize,
        out_channels: usize,
        stride: usize,
    ) -> Self {
        let conv1 = Conv2dConfig::new([in_channels, out_channels], [3, 3])
            .with_stride([stride, stride])
            .with_padding(PaddingConfig2d::Explicit(1, 1))
            .init(device);

        let bn1 = BatchNormConfig::new(out_channels).init(device);

        let conv2 = Conv2dConfig::new([out_channels, out_channels], [3, 3])
            .with_padding(PaddingConfig2d::Explicit(1, 1))
            .init(device);

        let bn2 = BatchNormConfig::new(out_channels).init(device);

        let downsample = if stride != 1 || in_channels != out_channels {
            Some(DownsampleLayer::new(device, in_channels, out_channels, stride))
        } else {
            None
        };

        Self {
            conv1,
            bn1,
            conv2,
            bn2,
            downsample,
        }
    }

    pub fn forward(&self, x: Tensor<B, 4>) -> Tensor<B, 4> {
        let identity = x.clone();

        let out = self.conv1.forward(x);
        let out = self.bn1.forward(out);
        let out = relu(out);

        let out = self.conv2.forward(out);
        let out = self.bn2.forward(out);

        let identity = if let Some(ref ds) = self.downsample {
            ds.forward(identity)
        } else {
            identity
        };

        relu(out + identity)
    }
}

#[derive(Module, Debug)]
pub struct ResidualLayer<B: Backend> {
    block0: BasicBlock<B>,
    block1: BasicBlock<B>,
}

impl<B: Backend> ResidualLayer<B> {
    pub fn new(device: &B::Device, in_channels: usize, out_channels: usize, stride: usize) -> Self {
        let block0 = BasicBlock::new(device, in_channels, out_channels, stride);
        let block1 = BasicBlock::new(device, out_channels, out_channels, 1);
        Self { block0, block1 }
    }

    pub fn forward(&self, x: Tensor<B, 4>) -> Tensor<B, 4> {
        let x = self.block0.forward(x);
        self.block1.forward(x)
    }
}

#[derive(Module, Debug)]
pub struct ResNet18<B: Backend> {
    conv1: Conv2d<B>,
    bn1: BatchNorm<B>,

    layer1: ResidualLayer<B>,
    layer2: ResidualLayer<B>,
    layer3: ResidualLayer<B>,
    layer4: ResidualLayer<B>,

    avgpool: AdaptiveAvgPool2d,
}

impl<B: Backend> ResNet18<B> {
    pub const OUTPUT_DIM: usize = 512;

    pub fn new(device: &B::Device) -> Self {
        let conv1 = Conv2dConfig::new([1, 64], [3, 3])
            .with_padding(PaddingConfig2d::Explicit(1, 1))
            .init(device);
        let bn1 = BatchNormConfig::new(64).init(device);

        let layer1 = ResidualLayer::new(device, 64, 64, 1);

        let layer2 = ResidualLayer::new(device, 64, 128, 2);

        let layer3 = ResidualLayer::new(device, 128, 256, 2);

        let layer4 = ResidualLayer::new(device, 256, 512, 2);

        let avgpool = AdaptiveAvgPool2dConfig::new([1, 1]).init();

        Self {
            conv1,
            bn1,
            layer1,
            layer2,
            layer3,
            layer4,
            avgpool,
        }
    }

    pub fn forward(&self, x: Tensor<B, 4>) -> Tensor<B, 2> {
        let x = self.conv1.forward(x);
        let x = self.bn1.forward(x);
        let x = relu(x);

        let x = self.layer1.forward(x);
        let x = self.layer2.forward(x);
        let x = self.layer3.forward(x);
        let x = self.layer4.forward(x);

        let x = self.avgpool.forward(x);

        let [batch, channels, _, _] = x.dims();
        x.reshape([batch, channels])
    }
}

#[derive(Module, Debug)]
pub struct ResNet18Lite<B: Backend> {
    conv1: Conv2d<B>,
    bn1: BatchNorm<B>,
    layer1: ResidualLayer<B>,
    layer2: ResidualLayer<B>,
    layer3: ResidualLayer<B>,
    avgpool: AdaptiveAvgPool2d,
}

impl<B: Backend> ResNet18Lite<B> {
    pub const OUTPUT_DIM: usize = 256;

    pub fn new(device: &B::Device) -> Self {
        let conv1 = Conv2dConfig::new([1, 32], [3, 3])
            .with_padding(PaddingConfig2d::Explicit(1, 1))
            .init(device);
        let bn1 = BatchNormConfig::new(32).init(device);

        let layer1 = ResidualLayer::new(device, 32, 32, 1);
        let layer2 = ResidualLayer::new(device, 32, 64, 2);
        let layer3 = ResidualLayer::new(device, 64, 256, 2);

        let avgpool = AdaptiveAvgPool2dConfig::new([1, 1]).init();

        Self {
            conv1,
            bn1,
            layer1,
            layer2,
            layer3,
            avgpool,
        }
    }

    pub fn forward(&self, x: Tensor<B, 4>) -> Tensor<B, 2> {
        let x = self.conv1.forward(x);
        let x = self.bn1.forward(x);
        let x = relu(x);

        let x = self.layer1.forward(x);
        let x = self.layer2.forward(x);
        let x = self.layer3.forward(x);

        let x = self.avgpool.forward(x);
        let [batch, channels, _, _] = x.dims();
        x.reshape([batch, channels])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn_ndarray::NdArray;

    type TestBackend = NdArray<f32>;

    #[test]
    fn test_basic_block_dimensions() {
        let device = Default::default();
        let block = BasicBlock::<TestBackend>::new(&device, 64, 64, 1);

        let input = Tensor::zeros([1, 64, 48, 48], &device);
        let output = block.forward(input);

        assert_eq!(output.dims(), [1, 64, 48, 48]);
    }

    #[test]
    fn test_basic_block_downsample() {
        let device = Default::default();
        let block = BasicBlock::<TestBackend>::new(&device, 64, 128, 2);

        let input = Tensor::zeros([1, 64, 48, 48], &device);
        let output = block.forward(input);

        assert_eq!(output.dims(), [1, 128, 24, 24]);
    }

    #[test]
    fn test_resnet18_forward() {
        let device = Default::default();
        let model = ResNet18::<TestBackend>::new(&device);

        let input = Tensor::zeros([2, 1, 192, 192], &device);
        let output = model.forward(input);

        assert_eq!(output.dims(), [2, 512]);
    }

    #[test]
    fn test_resnet18_lite_forward() {
        let device = Default::default();
        let model = ResNet18Lite::<TestBackend>::new(&device);

        let input = Tensor::zeros([2, 1, 192, 192], &device);
        let output = model.forward(input);

        assert_eq!(output.dims(), [2, 256]);
    }
}
