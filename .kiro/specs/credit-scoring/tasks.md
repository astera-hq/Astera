# Implementation Plan

- [x] 1. Fix average_payment_days running average bug
  - The current `record_payment` and `record_default` pass a pre-multiplied `total_late_days` to `calculate_average_payment_days`, but the multiplication uses `total_invoices - 1` after the increment, which is off-by-one for defaults (defaults don't contribute to the paid count used in the average denominator). Audit and correct the running average accumulation so it only counts paid invoices (on-time + late) in the denominator, not defaults.
  - _Requirements: 5.5_

- [ ]* 1.1 Write property test for running average correctness
  - **Property 6: Running average correctness**
  - **Validates: Requirements 5.5**

- [x] 2. Strengthen score bounds and formula unit tests
  - [x] 2.1 Write property test for score bounds invariant
    - **Property 1: Score bounds invariant**
    - **Validates: Requirements 1.5, 1.6**

  - [x] 2.2 Write property test for scoring formula monotonicity
    - **Property 2: Scoring formula monotonicity**
    - **Validates: Requirements 1.2, 1.3, 1.4**

  - [x] 2.3 Write property test for defaults-dominate rule
    - **Property 3: Defaults dominate — score below BASE when defaults exceed on-time**
    - **Validates: Requirements 4.1**

- [x] 3. Implement and test accumulation invariants
  - [x] 3.1 Write property test for invoice count accumulation
    - **Property 4: Invoice count accumulation invariant**
    - **Validates: Requirements 2.4**

  - [x] 3.2 Write property test for volume accumulation
    - **Property 5: Volume accumulation invariant**
    - **Validates: Requirements 3.4**

- [x] 4. Implement and test score band coverage
  - [x] 4.1 Write property test for score band coverage
    - **Property 7: Score band coverage**
    - **Validates: Requirements 6.1, 6.2, 6.3, 6.4, 6.5, 6.6**

- [x] 5. Implement and test payment history ordering
  - [x] 5.1 Write property test for payment history ordering invariant
    - **Property 8: Payment history ordering invariant**
    - **Validates: Requirements 7.1, 7.2, 7.3**

- [x] 6. Implement and test idempotency guard
  - [x] 6.1 Write property test for idempotency guard
    - **Property 9: Idempotency guard**
    - **Validates: Requirements 4.3**

- [x] 7. Checkpoint — Ensure all tests pass, ask the user if questions arise.
