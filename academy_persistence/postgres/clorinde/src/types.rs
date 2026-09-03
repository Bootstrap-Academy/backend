// This file was generated with `clorinde`. Do not modify.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum ContractDeclarationKind {
    cancellation,
    withdrawal,
}
impl<'a> postgres_types::ToSql for ContractDeclarationKind {
    fn to_sql(
        &self,
        ty: &postgres_types::Type,
        buf: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        let s = match *self {
            ContractDeclarationKind::cancellation => "cancellation",
            ContractDeclarationKind::withdrawal => "withdrawal",
        };
        buf.extend_from_slice(s.as_bytes());
        std::result::Result::Ok(postgres_types::IsNull::No)
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "contract_declaration_kind" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 2 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "cancellation" => true,
                    "withdrawal" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
    fn to_sql_checked(
        &self,
        ty: &postgres_types::Type,
        out: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        postgres_types::__to_sql_checked(self, ty, out)
    }
}
impl<'a> postgres_types::FromSql<'a> for ContractDeclarationKind {
    fn from_sql(
        ty: &postgres_types::Type,
        buf: &'a [u8],
    ) -> Result<ContractDeclarationKind, Box<dyn std::error::Error + Sync + Send>> {
        match std::str::from_utf8(buf)? {
            "cancellation" => Ok(ContractDeclarationKind::cancellation),
            "withdrawal" => Ok(ContractDeclarationKind::withdrawal),
            s => Result::Err(Into::into(format!("invalid variant `{}`", s))),
        }
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "contract_declaration_kind" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 2 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "cancellation" => true,
                    "withdrawal" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum ContractDeclarationContract {
    premium,
    coins,
    other,
}
impl<'a> postgres_types::ToSql for ContractDeclarationContract {
    fn to_sql(
        &self,
        ty: &postgres_types::Type,
        buf: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        let s = match *self {
            ContractDeclarationContract::premium => "premium",
            ContractDeclarationContract::coins => "coins",
            ContractDeclarationContract::other => "other",
        };
        buf.extend_from_slice(s.as_bytes());
        std::result::Result::Ok(postgres_types::IsNull::No)
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "contract_declaration_contract" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 3 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "premium" => true,
                    "coins" => true,
                    "other" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
    fn to_sql_checked(
        &self,
        ty: &postgres_types::Type,
        out: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        postgres_types::__to_sql_checked(self, ty, out)
    }
}
impl<'a> postgres_types::FromSql<'a> for ContractDeclarationContract {
    fn from_sql(
        ty: &postgres_types::Type,
        buf: &'a [u8],
    ) -> Result<ContractDeclarationContract, Box<dyn std::error::Error + Sync + Send>> {
        match std::str::from_utf8(buf)? {
            "premium" => Ok(ContractDeclarationContract::premium),
            "coins" => Ok(ContractDeclarationContract::coins),
            "other" => Ok(ContractDeclarationContract::other),
            s => Result::Err(Into::into(format!("invalid variant `{}`", s))),
        }
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "contract_declaration_contract" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 3 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "premium" => true,
                    "coins" => true,
                    "other" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum ContractCancellationType {
    ordinary,
    extraordinary,
}
impl<'a> postgres_types::ToSql for ContractCancellationType {
    fn to_sql(
        &self,
        ty: &postgres_types::Type,
        buf: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        let s = match *self {
            ContractCancellationType::ordinary => "ordinary",
            ContractCancellationType::extraordinary => "extraordinary",
        };
        buf.extend_from_slice(s.as_bytes());
        std::result::Result::Ok(postgres_types::IsNull::No)
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "contract_cancellation_type" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 2 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "ordinary" => true,
                    "extraordinary" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
    fn to_sql_checked(
        &self,
        ty: &postgres_types::Type,
        out: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        postgres_types::__to_sql_checked(self, ty, out)
    }
}
impl<'a> postgres_types::FromSql<'a> for ContractCancellationType {
    fn from_sql(
        ty: &postgres_types::Type,
        buf: &'a [u8],
    ) -> Result<ContractCancellationType, Box<dyn std::error::Error + Sync + Send>> {
        match std::str::from_utf8(buf)? {
            "ordinary" => Ok(ContractCancellationType::ordinary),
            "extraordinary" => Ok(ContractCancellationType::extraordinary),
            s => Result::Err(Into::into(format!("invalid variant `{}`", s))),
        }
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "contract_cancellation_type" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 2 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "ordinary" => true,
                    "extraordinary" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum PremiumPlan {
    monthly,
    yearly,
}
impl<'a> postgres_types::ToSql for PremiumPlan {
    fn to_sql(
        &self,
        ty: &postgres_types::Type,
        buf: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        let s = match *self {
            PremiumPlan::monthly => "monthly",
            PremiumPlan::yearly => "yearly",
        };
        buf.extend_from_slice(s.as_bytes());
        std::result::Result::Ok(postgres_types::IsNull::No)
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "premium_plan" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 2 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "monthly" => true,
                    "yearly" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
    fn to_sql_checked(
        &self,
        ty: &postgres_types::Type,
        out: &mut postgres_types::private::BytesMut,
    ) -> Result<postgres_types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        postgres_types::__to_sql_checked(self, ty, out)
    }
}
impl<'a> postgres_types::FromSql<'a> for PremiumPlan {
    fn from_sql(
        ty: &postgres_types::Type,
        buf: &'a [u8],
    ) -> Result<PremiumPlan, Box<dyn std::error::Error + Sync + Send>> {
        match std::str::from_utf8(buf)? {
            "monthly" => Ok(PremiumPlan::monthly),
            "yearly" => Ok(PremiumPlan::yearly),
            s => Result::Err(Into::into(format!("invalid variant `{}`", s))),
        }
    }
    fn accepts(ty: &postgres_types::Type) -> bool {
        if ty.name() != "premium_plan" {
            return false;
        }
        match *ty.kind() {
            postgres_types::Kind::Enum(ref variants) => {
                if variants.len() != 2 {
                    return false;
                }
                variants.iter().all(|v| match &**v {
                    "monthly" => true,
                    "yearly" => true,
                    _ => false,
                })
            }
            _ => false,
        }
    }
}
