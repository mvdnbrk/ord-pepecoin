use {super::*, ord::subcommand::epochs::Output, ord::Sat};

#[test]
fn empty() {
  assert_eq!(
    CommandBuilder::new("epochs").output::<Output>(),
    Output {
      starting_sats: vec![
        Sat(0),
        Sat(100000000000 * u128::from(COIN_VALUE)),
        Sat(122500000000 * u128::from(COIN_VALUE)),
        Sat(136250000000 * u128::from(COIN_VALUE)),
        Sat(148750000000 * u128::from(COIN_VALUE)),
        Sat(155000000000 * u128::from(COIN_VALUE)),
        Sat(158125000000 * u128::from(COIN_VALUE)),
        Sat(159687500000 * u128::from(COIN_VALUE)),
      ]
    }
  );
}
