use bdk_wallet::Balance as BdkBalance;
use bitcoin::Amount;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeRate(bitcoin::FeeRate);

impl FeeRate {
    pub fn from_sat_per_vb(sat_vb: u64) -> Option<Self> {
        bitcoin::FeeRate::from_sat_per_vb(sat_vb).map(Self)
    }

    pub(crate) fn into_inner(self) -> bitcoin::FeeRate {
        self.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Balance {
    pub confirmed: Amount,
    pub trusted_pending: Amount,
    pub untrusted_pending: Amount,
    pub immature: Amount,
}

impl Balance {
    pub fn total(&self) -> Amount {
        self.confirmed + self.trusted_pending + self.untrusted_pending + self.immature
    }

    pub fn spendable(&self) -> Amount {
        self.confirmed + self.trusted_pending
    }
}

impl From<BdkBalance> for Balance {
    fn from(b: BdkBalance) -> Self {
        Self {
            confirmed: b.confirmed,
            trusted_pending: b.trusted_pending,
            untrusted_pending: b.untrusted_pending,
            immature: b.immature,
        }
    }
}
