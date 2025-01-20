// This file was generated with `clorinde`. Do not modify.

#[cfg(feature = "chrono")]
pub mod time {
    pub type Timestamp = chrono::NaiveDateTime;
    pub type TimestampTz = chrono::DateTime<chrono::FixedOffset>;
    pub type Date = chrono::NaiveDate;
    pub type Time = chrono::NaiveTime;
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
