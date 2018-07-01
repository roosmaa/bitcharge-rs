use bigdecimal::BigDecimal;

pub struct Database {
    charges: Vec<Charge>,
}

impl Database {
    pub fn new(charges: Vec<Charge>) -> Self {
        Self{
            charges,
        }
    }
    pub fn get_charge_by_id(&self, charge_id: u64) -> Option<&Charge> {
        let mut it = self.charges.iter()
            .filter(|c| c.id == charge_id);
        it.next()
    }

    pub fn charges(&self) -> &[Charge] {
        self.charges.as_slice()
    }
}

#[derive(Debug)]
pub struct Charge {
    pub id: u64,
    pub invoice_id: String,
    pub eur_amount: BigDecimal,
    pub btc_address: String,
}


