/// SIMD-accelerated vector operations.
///
/// Provides cosine similarity/distance with:
///   - AVX2 (x86_64, 8-wide f32)
///   - NEON (aarch64, 4-wide f32)
///   - Scalar fallback (all architectures)
///
/// The public API auto-selects the best implementation at runtime.

/// Cosine distance: 1.0 - cosine_similarity. Lower = more similar.
#[inline]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    1.0 - cosine_similarity(a, b)
}

/// Cosine similarity between two vectors.
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { cosine_similarity_avx2(a, b) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { cosine_similarity_neon(a, b) };
    }
    #[allow(unreachable_code)]
    cosine_similarity_scalar(a, b)
}

/// Batch cosine distances: compute distance from `query` to each vector in `vectors`.
/// Returns a Vec of (index, distance) pairs, sorted by distance ascending.
pub fn batch_cosine_distances(query: &[f32], vectors: &[&[f32]], limit: usize) -> Vec<(usize, f32)> {
    let mut results: Vec<(usize, f32)> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| (i, cosine_distance(query, v)))
        .collect();

    results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

/// Dot product of two vectors.
#[inline]
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { dot_product_avx2(a, b) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { dot_product_neon(a, b) };
    }
    #[allow(unreachable_code)]
    dot_product_scalar(a, b)
}

/// L2 (Euclidean) distance squared between two vectors.
#[inline]
pub fn l2_distance_sq(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { l2_distance_sq_avx2(a, b) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { l2_distance_sq_neon(a, b) };
    }
    #[allow(unreachable_code)]
    l2_distance_sq_scalar(a, b)
}

/// Normalize a vector in-place to unit length.
pub fn normalize(v: &mut [f32]) {
    let norm = dot_product(v, v).sqrt();
    if norm > f32::EPSILON {
        let inv = 1.0 / norm;
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

// ── Scalar implementations ──

fn cosine_similarity_scalar(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = (norm_a * norm_b).sqrt();
    if denom < f32::EPSILON {
        return 0.0;
    }
    dot / denom
}

fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let mut sum = 0.0f32;
    for i in 0..len {
        sum += a[i] * b[i];
    }
    sum
}

fn l2_distance_sq_scalar(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let mut sum = 0.0f32;
    for i in 0..len {
        let d = a[i] - b[i];
        sum += d * d;
    }
    sum
}

// ── AVX2 implementations ──

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn cosine_similarity_avx2(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    // SAFETY: caller guarantees AVX2+FMA via target_feature check
    unsafe {
        let len = a.len().min(b.len());
        let chunks = len / 8;
        let remainder = len % 8;

        let mut dot = _mm256_setzero_ps();
        let mut norm_a = _mm256_setzero_ps();
        let mut norm_b = _mm256_setzero_ps();

        for i in 0..chunks {
            let offset = i * 8;
            let va = _mm256_loadu_ps(a.as_ptr().add(offset));
            let vb = _mm256_loadu_ps(b.as_ptr().add(offset));
            dot = _mm256_fmadd_ps(va, vb, dot);
            norm_a = _mm256_fmadd_ps(va, va, norm_a);
            norm_b = _mm256_fmadd_ps(vb, vb, norm_b);
        }

        let mut dot_sum = hsum_avx2(dot);
        let mut na_sum = hsum_avx2(norm_a);
        let mut nb_sum = hsum_avx2(norm_b);

        // Handle remainder
        let start = chunks * 8;
        for i in 0..remainder {
            let ai = a[start + i];
            let bi = b[start + i];
            dot_sum += ai * bi;
            na_sum += ai * ai;
            nb_sum += bi * bi;
        }

        let denom = (na_sum * nb_sum).sqrt();
        if denom < f32::EPSILON {
            return 0.0;
        }
        dot_sum / denom
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    // SAFETY: caller guarantees AVX2+FMA via target_feature check
    unsafe {
        let len = a.len().min(b.len());
        let chunks = len / 8;
        let remainder = len % 8;

        let mut acc = _mm256_setzero_ps();

        for i in 0..chunks {
            let offset = i * 8;
            let va = _mm256_loadu_ps(a.as_ptr().add(offset));
            let vb = _mm256_loadu_ps(b.as_ptr().add(offset));
            acc = _mm256_fmadd_ps(va, vb, acc);
        }

        let mut sum = hsum_avx2(acc);
        let start = chunks * 8;
        for i in 0..remainder {
            sum += a[start + i] * b[start + i];
        }
        sum
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn l2_distance_sq_avx2(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    // SAFETY: caller guarantees AVX2+FMA via target_feature check
    unsafe {
        let len = a.len().min(b.len());
        let chunks = len / 8;
        let remainder = len % 8;

        let mut acc = _mm256_setzero_ps();

        for i in 0..chunks {
            let offset = i * 8;
            let va = _mm256_loadu_ps(a.as_ptr().add(offset));
            let vb = _mm256_loadu_ps(b.as_ptr().add(offset));
            let diff = _mm256_sub_ps(va, vb);
            acc = _mm256_fmadd_ps(diff, diff, acc);
        }

        let mut sum = hsum_avx2(acc);
        let start = chunks * 8;
        for i in 0..remainder {
            let d = a[start + i] - b[start + i];
            sum += d * d;
        }
        sum
    }
}

/// Horizontal sum of an AVX2 256-bit register (8 x f32 → single f32).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[allow(unused_unsafe)]
unsafe fn hsum_avx2(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;

    unsafe {
        // Add high 128 to low 128
        let hi = _mm256_extractf128_ps(v, 1);
        let lo = _mm256_castps256_ps128(v);
        let sum128 = _mm_add_ps(lo, hi);

        // Horizontal add within 128-bit lane
        let shuf = _mm_movehdup_ps(sum128); // [1,1,3,3]
        let sums = _mm_add_ps(sum128, shuf); // [0+1, _, 2+3, _]
        let shuf2 = _mm_movehl_ps(sums, sums); // [2+3, _, _, _]
        let result = _mm_add_ss(sums, shuf2); // [0+1+2+3, _, _, _]
        _mm_cvtss_f32(result)
    }
}

// ── NEON implementations (aarch64) ──

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn cosine_similarity_neon(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::aarch64::*;

    let len = a.len().min(b.len());
    let chunks = len / 4;
    let remainder = len % 4;

    let mut dot = vdupq_n_f32(0.0);
    let mut norm_a = vdupq_n_f32(0.0);
    let mut norm_b = vdupq_n_f32(0.0);

    for i in 0..chunks {
        let offset = i * 4;
        let va = vld1q_f32(a.as_ptr().add(offset));
        let vb = vld1q_f32(b.as_ptr().add(offset));
        dot = vfmaq_f32(dot, va, vb);
        norm_a = vfmaq_f32(norm_a, va, va);
        norm_b = vfmaq_f32(norm_b, vb, vb);
    }

    let mut dot_sum = vaddvq_f32(dot);
    let mut na_sum = vaddvq_f32(norm_a);
    let mut nb_sum = vaddvq_f32(norm_b);

    // Handle remainder
    let start = chunks * 4;
    for i in 0..remainder {
        let ai = a[start + i];
        let bi = b[start + i];
        dot_sum += ai * bi;
        na_sum += ai * ai;
        nb_sum += bi * bi;
    }

    let denom = (na_sum * nb_sum).sqrt();
    if denom < f32::EPSILON {
        return 0.0;
    }
    dot_sum / denom
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_product_neon(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::aarch64::*;

    let len = a.len().min(b.len());
    let chunks = len / 4;
    let remainder = len % 4;

    let mut acc = vdupq_n_f32(0.0);

    for i in 0..chunks {
        let offset = i * 4;
        let va = vld1q_f32(a.as_ptr().add(offset));
        let vb = vld1q_f32(b.as_ptr().add(offset));
        acc = vfmaq_f32(acc, va, vb);
    }

    let mut sum = vaddvq_f32(acc);
    let start = chunks * 4;
    for i in 0..remainder {
        sum += a[start + i] * b[start + i];
    }
    sum
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn l2_distance_sq_neon(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::aarch64::*;

    let len = a.len().min(b.len());
    let chunks = len / 4;
    let remainder = len % 4;

    let mut acc = vdupq_n_f32(0.0);

    for i in 0..chunks {
        let offset = i * 4;
        let va = vld1q_f32(a.as_ptr().add(offset));
        let vb = vld1q_f32(b.as_ptr().add(offset));
        let diff = vsubq_f32(va, vb);
        acc = vfmaq_f32(acc, diff, diff);
    }

    let mut sum = vaddvq_f32(acc);
    let start = chunks * 4;
    for i in 0..remainder {
        let d = a[start + i] - b[start + i];
        sum += d * d;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 0.001, "got {sim}");
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0, 0.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001, "got {sim}");
    }

    #[test]
    fn cosine_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.001, "got {sim}");
    }

    #[test]
    fn cosine_distance_check() {
        let a = vec![1.0, 0.0, 0.0];
        let d = cosine_distance(&a, &a);
        assert!(d.abs() < 0.001, "got {d}");
    }

    #[test]
    fn dot_product_basic() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        let d = dot_product(&a, &b);
        assert!((d - 32.0).abs() < 0.001, "got {d}");
    }

    #[test]
    fn l2_distance_basic() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        let d = l2_distance_sq(&a, &b);
        assert!((d - 25.0).abs() < 0.001, "got {d}");
    }

    #[test]
    fn normalize_unit() {
        let mut v = vec![3.0, 4.0, 0.0];
        normalize(&mut v);
        let norm = dot_product(&v, &v).sqrt();
        assert!((norm - 1.0).abs() < 0.001, "got {norm}");
    }

    #[test]
    fn batch_distances() {
        let query = vec![1.0, 0.0, 0.0];
        let v1 = vec![1.0, 0.0, 0.0]; // identical
        let v2 = vec![0.0, 1.0, 0.0]; // orthogonal
        let v3 = vec![0.9, 0.1, 0.0]; // close

        let vectors: Vec<&[f32]> = vec![&v1, &v2, &v3];
        let results = batch_cosine_distances(&query, &vectors, 3);

        // v1 (identical) should be first, v3 (close) second, v2 (orthogonal) last
        assert_eq!(results[0].0, 0); // v1
        assert_eq!(results[2].0, 1); // v2
    }

    #[test]
    fn large_vector_simd() {
        // Test with a vector large enough to exercise SIMD paths (384-dim like MiniLM)
        let dim = 384;
        let a: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.01).collect();
        let b: Vec<f32> = (0..dim).map(|i| ((dim - i) as f32) * 0.01).collect();

        let sim = cosine_similarity(&a, &b);
        // Cross-check with scalar
        let sim_scalar = cosine_similarity_scalar(&a, &b);
        assert!(
            (sim - sim_scalar).abs() < 0.0001,
            "SIMD {sim} != scalar {sim_scalar}"
        );
    }

    #[test]
    fn large_dot_product() {
        let dim = 384;
        let a: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.01).collect();
        let b: Vec<f32> = (0..dim).map(|i| ((dim - i) as f32) * 0.01).collect();

        let d = dot_product(&a, &b);
        let d_scalar = dot_product_scalar(&a, &b);
        assert!(
            (d - d_scalar).abs() < 0.01,
            "SIMD {d} != scalar {d_scalar}"
        );
    }

    #[test]
    fn large_l2() {
        let dim = 384;
        let a: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.01).collect();
        let b: Vec<f32> = (0..dim).map(|i| ((dim - i) as f32) * 0.01).collect();

        let d = l2_distance_sq(&a, &b);
        let d_scalar = l2_distance_sq_scalar(&a, &b);
        assert!(
            (d - d_scalar).abs() < 0.01,
            "SIMD {d} != scalar {d_scalar}"
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_cosine_matches_scalar() {
        let dim = 384;
        let a: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.01).collect();
        let b: Vec<f32> = (0..dim).map(|i| ((dim - i) as f32) * 0.01).collect();

        let neon_result = unsafe { cosine_similarity_neon(&a, &b) };
        let scalar_result = cosine_similarity_scalar(&a, &b);
        assert!(
            (neon_result - scalar_result).abs() < 0.0001,
            "NEON {neon_result} != scalar {scalar_result}"
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_dot_product_matches_scalar() {
        let dim = 384;
        let a: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.01).collect();
        let b: Vec<f32> = (0..dim).map(|i| ((dim - i) as f32) * 0.01).collect();

        let neon_result = unsafe { dot_product_neon(&a, &b) };
        let scalar_result = dot_product_scalar(&a, &b);
        assert!(
            (neon_result - scalar_result).abs() < 0.01,
            "NEON {neon_result} != scalar {scalar_result}"
        );
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn neon_l2_matches_scalar() {
        let dim = 384;
        let a: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.01).collect();
        let b: Vec<f32> = (0..dim).map(|i| ((dim - i) as f32) * 0.01).collect();

        let neon_result = unsafe { l2_distance_sq_neon(&a, &b) };
        let scalar_result = l2_distance_sq_scalar(&a, &b);
        assert!(
            (neon_result - scalar_result).abs() < 0.01,
            "NEON {neon_result} != scalar {scalar_result}"
        );
    }
}
