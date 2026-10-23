use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};
use crate::ids::{CustomerId, SellerId, StoreId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    Male,
    Female,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Address {
    pub street: String,
    pub number: String,
    pub locality: String,
    pub postal_code: String,
    pub province: String,
}

impl Address {
    #[must_use]
    pub fn one_line(&self) -> String {
        [
            self.street.as_str(),
            self.number.as_str(),
            self.postal_code.as_str(),
            self.locality.as_str(),
            self.province.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer {
    pub id: CustomerId,
    pub first_name: String,
    pub last_name1: String,
    pub last_name2: Option<String>,
    pub sex: Option<Sex>,
    pub phone1: Option<String>,
    pub phone2: Option<String>,
    pub email: Option<String>,
    pub address: Address,
    pub nif: Option<String>,
    pub profession: Option<String>,
    pub notes: Option<String>,
    pub active: bool,
}

impl Customer {
    pub fn new(
        id: CustomerId,
        first_name: impl Into<String>,
        last_name1: impl Into<String>,
    ) -> Result<Self> {
        let first_name = first_name.into().trim().to_string();
        let last_name1 = last_name1.into().trim().to_string();
        if first_name.is_empty() || last_name1.is_empty() {
            return Err(DomainError::Validation(
                "customer name and first surname are required".into(),
            ));
        }
        Ok(Self {
            id,
            first_name,
            last_name1,
            last_name2: None,
            sex: None,
            phone1: None,
            phone2: None,
            email: None,
            address: Address::default(),
            nif: None,
            profession: None,
            notes: None,
            active: true,
        })
    }

    #[must_use]
    pub fn full_name(&self) -> String {
        [
            Some(self.first_name.as_str()),
            Some(self.last_name1.as_str()),
            self.last_name2.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
    }

    /// A complete invoice requires a valid Spanish tax identifier.
    pub fn validate_for_invoice(&self) -> Result<()> {
        let nif = self.nif.as_deref().ok_or(DomainError::InvalidTaxId)?;
        if validate_spanish_tax_id(nif) {
            Ok(())
        } else {
            Err(DomainError::InvalidTaxId)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seller {
    pub id: SellerId,
    pub code: String,
    pub name: String,
    pub phone1: Option<String>,
    pub phone2: Option<String>,
    pub active: bool,
}

impl Seller {
    pub fn new(id: SellerId, code: impl Into<String>, name: impl Into<String>) -> Result<Self> {
        let code = code.into().trim().to_string();
        let name = name.into().trim().to_string();
        if code.is_empty() || name.is_empty() {
            return Err(DomainError::Validation(
                "seller code and name are required".into(),
            ));
        }
        Ok(Self {
            id,
            code,
            name,
            phone1: None,
            phone2: None,
            active: true,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Store {
    pub id: StoreId,
    pub code: String,
    pub name: String,
    pub tax_id: String,
    pub phone: Option<String>,
    pub address: Address,
    pub active: bool,
}

impl Store {
    pub fn new(
        id: StoreId,
        code: impl Into<String>,
        name: impl Into<String>,
        tax_id: impl Into<String>,
    ) -> Result<Self> {
        let code = code.into().trim().to_string();
        let name = name.into().trim().to_string();
        let tax_id = tax_id.into().trim().to_uppercase();
        if code.is_empty() || name.is_empty() {
            return Err(DomainError::Validation(
                "store code and name are required".into(),
            ));
        }
        if !validate_spanish_tax_id(&tax_id) {
            return Err(DomainError::InvalidTaxId);
        }
        Ok(Self {
            id,
            code,
            name,
            tax_id,
            phone: None,
            address: Address::default(),
            active: true,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Manager,
    Cashier,
    Auditor,
}

impl Role {
    #[must_use]
    pub fn can_sell(self) -> bool {
        matches!(self, Self::Admin | Self::Manager | Self::Cashier)
    }

    #[must_use]
    pub fn can_manage_master_data(self) -> bool {
        matches!(self, Self::Admin | Self::Manager)
    }

    #[must_use]
    pub fn can_reopen_cash(self) -> bool {
        matches!(self, Self::Admin | Self::Manager)
    }

    #[must_use]
    pub fn can_rectify_invoice(self) -> bool {
        matches!(self, Self::Admin | Self::Manager)
    }

    #[must_use]
    pub fn can_view_audit(self) -> bool {
        matches!(self, Self::Admin | Self::Manager | Self::Auditor)
    }

    #[must_use]
    pub fn can_manage_users(self) -> bool {
        matches!(self, Self::Admin)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAccount {
    pub id: UserId,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub store_ids: Vec<StoreId>,
    pub active: bool,
}

impl UserAccount {
    pub fn new(
        id: UserId,
        username: impl Into<String>,
        display_name: impl Into<String>,
        role: Role,
    ) -> Result<Self> {
        let username = username.into().trim().to_lowercase();
        let display_name = display_name.into().trim().to_string();
        if username.is_empty() || display_name.is_empty() {
            return Err(DomainError::Validation(
                "username and display name are required".into(),
            ));
        }
        Ok(Self {
            id,
            username,
            display_name,
            role,
            store_ids: Vec::new(),
            active: true,
        })
    }

    pub fn ensure_store_access(&self, store_id: StoreId) -> Result<()> {
        if self.role == Role::Admin || self.store_ids.contains(&store_id) {
            Ok(())
        } else {
            Err(DomainError::Validation(
                "user does not have access to this store".into(),
            ))
        }
    }
}

/// User data and password hash provisioned from the central server.
///
/// The password hash is never exposed through `Debug`; it is still serialized
/// in the signed sync event so a terminal can recreate the local account.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserProvision {
    pub user: UserAccount,
    pub password_hash: String,
    pub revision: u64,
}

impl std::fmt::Debug for UserProvision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserProvision")
            .field("user", &self.user)
            .field("password_hash", &"<redacted>")
            .field("revision", &self.revision)
            .finish()
    }
}

/// Validates Spanish DNI, NIE and CIF identifiers.
///
/// This is intentionally a pure function so it can be reused by the UI and by
/// the backend without duplicating rules.
#[must_use]
pub fn validate_spanish_tax_id(value: &str) -> bool {
    let normalized = value.trim().to_uppercase().replace([' ', '-'], "");
    if normalized.len() != 9 {
        return false;
    }
    let chars: Vec<char> = normalized.chars().collect();
    match chars[0] {
        '0'..='9' => validate_dni(&chars),
        'X' | 'Y' | 'Z' => validate_nie(&chars),
        'A'..='Z' => validate_cif(&chars),
        _ => false,
    }
}

fn dni_letter(number: u64) -> char {
    const LETTERS: [char; 23] = [
        'T', 'R', 'W', 'A', 'G', 'M', 'Y', 'F', 'P', 'D', 'X', 'B', 'N', 'J', 'Z', 'S', 'Q', 'V',
        'H', 'L', 'C', 'K', 'E',
    ];
    LETTERS[(number % 23) as usize]
}

fn validate_dni(chars: &[char]) -> bool {
    let number: String = chars[..8].iter().collect();
    let Ok(number) = number.parse::<u64>() else {
        return false;
    };
    chars[8] == dni_letter(number)
}

fn validate_nie(chars: &[char]) -> bool {
    let prefix = match chars[0] {
        'X' => '0',
        'Y' => '1',
        'Z' => '2',
        _ => return false,
    };
    let number: String = std::iter::once(prefix)
        .chain(chars[1..8].iter().copied())
        .collect();
    let Ok(number) = number.parse::<u64>() else {
        return false;
    };
    chars[8] == dni_letter(number)
}

fn validate_cif(chars: &[char]) -> bool {
    let mut sum = 0u32;
    for (index, ch) in chars[1..8].iter().enumerate() {
        let Some(digit) = ch.to_digit(10) else {
            return false;
        };
        if index % 2 == 1 {
            sum += digit;
        } else {
            let doubled = digit * 2;
            sum += doubled / 10 + doubled % 10;
        }
    }
    let expected = (10 - (sum % 10)) % 10;
    let control = chars[8];
    if let Some(digit) = control.to_digit(10) {
        return digit == expected;
    }
    const CONTROL_LETTERS: [char; 10] = ['J', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I'];
    control == CONTROL_LETTERS[expected as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dni_nie_and_cif() {
        assert!(validate_spanish_tax_id("12345678Z"));
        assert!(validate_spanish_tax_id("X1234567L"));
        assert!(validate_spanish_tax_id("B12345674"));
        assert!(!validate_spanish_tax_id("12345678A"));
        assert!(!validate_spanish_tax_id(""));
    }

    #[test]
    fn invoice_requires_valid_nif() {
        let mut customer = Customer::new(CustomerId::new(), "Ana", "García").unwrap();
        assert!(customer.validate_for_invoice().is_err());
        customer.nif = Some("12345678Z".into());
        assert!(customer.validate_for_invoice().is_ok());
    }

    #[test]
    fn role_permissions_are_explicit() {
        assert!(Role::Cashier.can_sell());
        assert!(!Role::Cashier.can_reopen_cash());
        assert!(Role::Manager.can_reopen_cash());
        assert!(Role::Auditor.can_view_audit());
        assert!(Role::Admin.can_manage_users());
        assert!(!Role::Manager.can_manage_users());
        assert!(!Role::Cashier.can_manage_users());
        assert!(!Role::Auditor.can_manage_users());
    }
}
