// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

use crate::{error::Error, proof_system::ProverKey};
use ark_ec::TEModelParameters;
use ark_ff::PrimeField;
use ark_poly::{
    univariate::DensePolynomial, EvaluationDomain, GeneralEvaluationDomain,
    UVPolynomial,
};

/// Computes the Quotient [`DensePolynomial`] given the [`EvaluationDomain`], a
/// [`ProverKey`] and some other info.
pub(crate) fn compute<F: PrimeField, P: TEModelParameters<BaseField = F>>(
    domain: &GeneralEvaluationDomain<F>,
    prover_key: &ProverKey<F, P>,
    z_poly: &DensePolynomial<F>,
    (w_l_poly, w_r_poly, w_o_poly, w_4_poly): (
        &DensePolynomial<F>,
        &DensePolynomial<F>,
        &DensePolynomial<F>,
        &DensePolynomial<F>,
    ),
    public_inputs_poly: &DensePolynomial<F>,
    (
        alpha,
        beta,
        gamma,
        range_challenge,
        logic_challenge,
        fixed_base_challenge,
        var_base_challenge,
    ): &(F, F, F, F, F, F, F),
) -> Result<DensePolynomial<F>, Error> {
    // Compute 4n eval of z(X)
    let domain_4n =
        GeneralEvaluationDomain::<F>::new(4 * domain.size()).unwrap();
    let mut z_eval_4n = domain_4n.coset_fft(z_poly);
    z_eval_4n.push(z_eval_4n[0]);
    z_eval_4n.push(z_eval_4n[1]);
    z_eval_4n.push(z_eval_4n[2]);
    z_eval_4n.push(z_eval_4n[3]);

    // Compute 4n evaluations of the wire Densepolynomials
    let mut wl_eval_4n = domain_4n.coset_fft(w_l_poly);
    wl_eval_4n.push(wl_eval_4n[0]);
    wl_eval_4n.push(wl_eval_4n[1]);
    wl_eval_4n.push(wl_eval_4n[2]);
    wl_eval_4n.push(wl_eval_4n[3]);
    let mut wr_eval_4n = domain_4n.coset_fft(w_r_poly);
    wr_eval_4n.push(wr_eval_4n[0]);
    wr_eval_4n.push(wr_eval_4n[1]);
    wr_eval_4n.push(wr_eval_4n[2]);
    wr_eval_4n.push(wr_eval_4n[3]);
    let wo_eval_4n = domain_4n.coset_fft(w_o_poly);

    let mut w4_eval_4n = domain_4n.coset_fft(w_4_poly);
    w4_eval_4n.push(w4_eval_4n[0]);
    w4_eval_4n.push(w4_eval_4n[1]);
    w4_eval_4n.push(w4_eval_4n[2]);
    w4_eval_4n.push(w4_eval_4n[3]);

    let t_1 = compute_circuit_satisfiability_equation(
        domain,
        (
            *range_challenge,
            *logic_challenge,
            *fixed_base_challenge,
            *var_base_challenge,
        ),
        prover_key,
        (&wl_eval_4n, &wr_eval_4n, &wo_eval_4n, &w4_eval_4n),
        public_inputs_poly,
    );

    let t_2 = compute_permutation_checks(
        domain,
        prover_key,
        (&wl_eval_4n, &wr_eval_4n, &wo_eval_4n, &w4_eval_4n),
        &z_eval_4n,
        (*alpha, *beta, *gamma),
    );
    let range = 0..domain_4n.size();

    let quotient: Vec<_> = range
        .map(|i| {
            let numerator = t_1[i] + t_2[i];
            let denominator = prover_key.v_h_coset_4n()[i];
            numerator * denominator.inverse().unwrap()
        })
        .collect();

    Ok(DensePolynomial {
        coeffs: domain_4n.coset_ifft(&quotient),
    })
}

// Ensures that the circuit is satisfied
fn compute_circuit_satisfiability_equation<
    F: PrimeField,
    P: TEModelParameters<BaseField = F>,
>(
    domain: &GeneralEvaluationDomain<F>,
    (
        range_challenge,
        logic_challenge,
        fixed_base_challenge,
        var_base_challenge,
    ): (F, F, F, F),
    prover_key: &ProverKey<F, P>,
    (wl_eval_4n, wr_eval_4n, wo_eval_4n, w4_eval_4n): (&[F], &[F], &[F], &[F]),
    pi_poly: &DensePolynomial<F>,
) -> Vec<F> {
    let domain_4n =
        GeneralEvaluationDomain::<F>::new(4 * domain.size()).unwrap();
    let pi_eval_4n = domain_4n.coset_fft(pi_poly);

    let range = 0..domain_4n.size();

    let t: Vec<_> = range
        .map(|i| {
            let wl = wl_eval_4n[i];
            let wr = wr_eval_4n[i];
            let wo = wo_eval_4n[i];
            let w4 = w4_eval_4n[i];
            let wl_next = wl_eval_4n[i + 4];
            let wr_next = wr_eval_4n[i + 4];
            let w4_next = w4_eval_4n[i + 4];
            let pi = pi_eval_4n[i];

            let a = prover_key.arithmetic.compute_quotient_i(i, wl, wr, wo, w4);

            let b = prover_key.range.compute_quotient_i(
                i,
                range_challenge,
                wl,
                wr,
                wo,
                w4,
                w4_next,
            );

            let c = prover_key.logic.compute_quotient_i(
                i,
                logic_challenge,
                wl,
                wl_next,
                wr,
                wr_next,
                wo,
                w4,
                w4_next,
            );

            let d = prover_key.fixed_base.compute_quotient_i(
                i,
                fixed_base_challenge,
                wl,
                wl_next,
                wr,
                wr_next,
                wo,
                w4,
                w4_next,
            );

            let e = prover_key.variable_base.compute_quotient_i(
                i,
                var_base_challenge,
                wl,
                wl_next,
                wr,
                wr_next,
                wo,
                w4,
                w4_next,
            );

            (a + pi) + b + c + d + e
        })
        .collect();
    t
}

fn compute_permutation_checks<
    F: PrimeField,
    P: TEModelParameters<BaseField = F>,
>(
    domain: &GeneralEvaluationDomain<F>,
    prover_key: &ProverKey<F, P>,
    (wl_eval_4n, wr_eval_4n, wo_eval_4n, w4_eval_4n): (&[F], &[F], &[F], &[F]),
    z_eval_4n: &[F],
    (alpha, beta, gamma): (F, F, F),
) -> Vec<F> {
    let domain_4n =
        GeneralEvaluationDomain::<F>::new(4 * domain.size()).unwrap();
    let l1_poly_alpha =
        compute_first_lagrange_poly_scaled(domain, alpha.square());
    let l1_alpha_sq_evals = domain_4n.coset_fft(&l1_poly_alpha.coeffs);

    let range = 0..domain_4n.size();

    let t: Vec<_> = range
        .map(|i| {
            prover_key.permutation.compute_quotient_i(
                i,
                wl_eval_4n[i],
                wr_eval_4n[i],
                wo_eval_4n[i],
                w4_eval_4n[i],
                z_eval_4n[i],
                z_eval_4n[i + 4],
                alpha,
                l1_alpha_sq_evals[i],
                beta,
                gamma,
            )
        })
        .collect();
    t
}
fn compute_first_lagrange_poly_scaled<F: PrimeField>(
    domain: &GeneralEvaluationDomain<F>,
    scale: F,
) -> DensePolynomial<F> {
    let mut x_evals = vec![F::zero(); domain.size()];
    x_evals[0] = scale;
    domain.ifft_in_place(&mut x_evals);
    DensePolynomial::from_coefficients_vec(x_evals)
}
