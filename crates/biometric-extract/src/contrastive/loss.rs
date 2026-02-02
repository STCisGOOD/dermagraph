
use burn::prelude::*;

pub struct ContrastiveBatch<B: Backend> {
    pub anchor_images: Tensor<B, 4>,
    pub positive_images: Tensor<B, 4>,
    pub anchor_classical: Tensor<B, 2>,
    pub positive_classical: Tensor<B, 2>,
    pub person_ids: Vec<usize>,
}

pub fn info_nce_loss<B: Backend>(
    anchor_embeddings: Tensor<B, 2>,
    positive_embeddings: Tensor<B, 2>,
    temperature: f32,
) -> Tensor<B, 1> {
    let [batch_size, _embed_dim] = anchor_embeddings.dims();
    let device = anchor_embeddings.device();

    let pos_sim = (anchor_embeddings.clone() * positive_embeddings.clone())
        .sum_dim(1) / temperature;

    let neg_sim_matrix = anchor_embeddings.matmul(positive_embeddings.transpose()) / temperature;

    let diag_mask = create_diagonal_mask::<B>(&device, batch_size);
    let neg_sim_masked = neg_sim_matrix - diag_mask * 1e9;

    let neg_max = neg_sim_masked.clone().max_dim(1);
    let neg_exp_sum = (neg_sim_masked - neg_max.clone()).exp().sum_dim(1);
    let neg_logsumexp = neg_max + neg_exp_sum.log();

    let pos_for_lse = pos_sim.clone();
    let max_val = pos_for_lse.clone().max_pair(neg_logsumexp.clone());
    let total_logsumexp = max_val.clone()
        + ((pos_for_lse - max_val.clone()).exp() + (neg_logsumexp - max_val).exp()).log();

    let nll = total_logsumexp - pos_sim;

    nll.mean()
}

fn create_diagonal_mask<B: Backend>(device: &B::Device, size: usize) -> Tensor<B, 2> {
    let mut mask_data = vec![0.0f32; size * size];
    for i in 0..size {
        mask_data[i * size + i] = 1.0;
    }
    Tensor::<B, 1>::from_floats(mask_data.as_slice(), device).reshape([size, size])
}

fn cross_entropy_with_logits<B: Backend>(
    logits: Tensor<B, 2>,
    targets: &[i64],
    device: &B::Device,
) -> Tensor<B, 1> {
    let [batch_size, num_classes] = logits.dims();

    let max_logits = logits.clone().max_dim(1);
    let shifted = logits - max_logits.clone();
    let log_sum_exp = shifted.clone().exp().sum_dim(1).log();
    let log_probs = shifted - log_sum_exp;

    let mut one_hot_data = vec![0.0f32; batch_size * num_classes];
    for (i, &target) in targets.iter().enumerate() {
        one_hot_data[i * num_classes + target as usize] = 1.0;
    }
    let one_hot: Tensor<B, 2> = Tensor::<B, 1>::from_floats(one_hot_data.as_slice(), device)
        .reshape([batch_size, num_classes]);

    let target_log_probs = (log_probs * one_hot).sum_dim(1);

    let nll = target_log_probs.neg();

    nll.mean()
}

pub fn contrastive_accuracy<B: Backend>(
    anchor_embeddings: Tensor<B, 2>,
    positive_embeddings: Tensor<B, 2>,
) -> f32 {
    let [batch_size, _] = anchor_embeddings.dims();

    if batch_size == 0 {
        return 0.0;
    }

    let similarity = anchor_embeddings.matmul(positive_embeddings.transpose());

    let argmax = similarity.argmax(1);

    let expected: Vec<i64> = (0..batch_size as i64).collect();
    let expected_tensor: Tensor<B, 2, burn::tensor::Int> =
        Tensor::<B, 1, burn::tensor::Int>::from_ints(expected.as_slice(), &argmax.device())
            .reshape([batch_size, 1]);

    let correct = argmax.equal(expected_tensor);
    let correct_float = correct.float();
    let acc_tensor = correct_float.mean();

    acc_tensor.into_scalar().elem()
}

pub fn find_hard_negatives<B: Backend>(
    embeddings: Tensor<B, 2>,
    person_ids: &[usize],
) -> Vec<usize> {
    let [batch_size, _] = embeddings.dims();

    let similarity = embeddings.clone().matmul(embeddings.transpose());
    let sim_data: Vec<f32> = similarity.into_data().to_vec().unwrap();

    let mut hard_negatives = Vec::with_capacity(batch_size);

    for i in 0..batch_size {
        let mut best_neg_idx = 0;
        let mut best_neg_sim = f32::NEG_INFINITY;

        for j in 0..batch_size {
            if i == j {
                continue;
            }

            if person_ids[j] == person_ids[i] {
                continue;
            }

            let sim = sim_data[i * batch_size + j];
            if sim > best_neg_sim {
                best_neg_sim = sim;
                best_neg_idx = j;
            }
        }

        hard_negatives.push(best_neg_idx);
    }

    hard_negatives
}

pub fn triplet_loss<B: Backend>(
    anchor: Tensor<B, 2>,
    positive: Tensor<B, 2>,
    negative: Tensor<B, 2>,
    margin: f32,
) -> Tensor<B, 1> {
    let pos_sim = (anchor.clone() * positive).sum_dim(1);
    let neg_sim = (anchor * negative).sum_dim(1);

    let diff = neg_sim - pos_sim + margin;
    let loss = diff.clamp_min(0.0);

    loss.mean()
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn_ndarray::NdArray;

    type TestBackend = NdArray<f32>;

    #[test]
    fn test_info_nce_identical() {
        let device = Default::default();

        let anchors = Tensor::<TestBackend, 2>::from_floats(
            [[1.0, 0.0], [0.0, 1.0]],
            &device,
        );
        let positives = anchors.clone();

        let loss = info_nce_loss(anchors, positives, 0.07);
        let loss_val: f32 = loss.into_data().to_vec().unwrap()[0];

        println!("InfoNCE loss (identical): {}", loss_val);
        assert!(loss_val < 2.0);
    }

    #[test]
    fn test_contrastive_accuracy() {
        let device = Default::default();

        let anchors = Tensor::<TestBackend, 2>::from_floats(
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            &device,
        );
        let positives = anchors.clone();

        let acc = contrastive_accuracy(anchors, positives);
        assert!((acc - 1.0).abs() < 1e-5, "Expected accuracy 1.0, got {}", acc);
    }

    #[test]
    fn test_triplet_loss() {
        let device = Default::default();

        let anchor = Tensor::<TestBackend, 2>::from_floats([[1.0, 0.0]], &device);
        let positive = Tensor::<TestBackend, 2>::from_floats([[0.9, 0.1]], &device);
        let negative = Tensor::<TestBackend, 2>::from_floats([[-1.0, 0.0]], &device);

        let loss = triplet_loss(anchor, positive, negative, 0.5);
        let loss_val: f32 = loss.into_data().to_vec().unwrap()[0];

        assert!(loss_val.abs() < 1e-5, "Expected ~0 loss, got {}", loss_val);
    }
}
