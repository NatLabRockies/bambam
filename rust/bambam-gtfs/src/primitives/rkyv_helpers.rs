//! `rkyv::with` adapters for external primitive types (`chrono::NaiveDate`, `chrono_tz::Tz`, and `rgb::RGB8`).

use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;
use rgb::RGB8;
use std::collections::HashSet;

/// `rkyv::with` wrapper that serializes `chrono::NaiveDate` as a compact `i32` (`num_days_from_ce`).
pub struct RkyvNaiveDate;

impl rkyv::with::ArchiveWith<NaiveDate> for RkyvNaiveDate {
    type Archived = rkyv::rend::i32_le;
    type Resolver = ();

    fn resolve_with(field: &NaiveDate, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        let days = field.num_days_from_ce();
        rkyv::Archive::resolve(&days, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + ?Sized> rkyv::with::SerializeWith<NaiveDate, S> for RkyvNaiveDate {
    fn serialize_with(field: &NaiveDate, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let days = field.num_days_from_ce();
        rkyv::Serialize::serialize(&days, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized>
    rkyv::with::DeserializeWith<rkyv::rend::i32_le, NaiveDate, D> for RkyvNaiveDate
{
    fn deserialize_with(
        field: &rkyv::rend::i32_le,
        _deserializer: &mut D,
    ) -> Result<NaiveDate, D::Error> {
        Ok(NaiveDate::from_num_days_from_ce_opt(field.to_native()).unwrap_or_default())
    }
}

/// `rkyv::with` wrapper that serializes `HashSet<NaiveDate>` as a sorted `Vec<i32>` (`num_days_from_ce`).
pub struct RkyvNaiveDateSet;

impl rkyv::with::ArchiveWith<HashSet<NaiveDate>> for RkyvNaiveDateSet {
    type Archived = rkyv::vec::ArchivedVec<rkyv::rend::i32_le>;
    type Resolver = rkyv::vec::VecResolver;

    fn resolve_with(
        field: &HashSet<NaiveDate>,
        resolver: Self::Resolver,
        out: rkyv::Place<Self::Archived>,
    ) {
        let mut days: Vec<i32> = field.iter().map(|d| d.num_days_from_ce()).collect();
        days.sort_unstable();
        rkyv::vec::ArchivedVec::resolve_from_slice(&days, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + rkyv::ser::Allocator + rkyv::ser::Writer + ?Sized>
    rkyv::with::SerializeWith<HashSet<NaiveDate>, S> for RkyvNaiveDateSet
{
    fn serialize_with(
        field: &HashSet<NaiveDate>,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        let mut days: Vec<i32> = field.iter().map(|d| d.num_days_from_ce()).collect();
        days.sort_unstable();
        rkyv::vec::ArchivedVec::serialize_from_slice(&days, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized>
    rkyv::with::DeserializeWith<rkyv::vec::ArchivedVec<rkyv::rend::i32_le>, HashSet<NaiveDate>, D>
    for RkyvNaiveDateSet
{
    fn deserialize_with(
        field: &rkyv::vec::ArchivedVec<rkyv::rend::i32_le>,
        _deserializer: &mut D,
    ) -> Result<HashSet<NaiveDate>, D::Error> {
        Ok(field
            .as_slice()
            .iter()
            .filter_map(|d| NaiveDate::from_num_days_from_ce_opt(d.to_native()))
            .collect())
    }
}

/// `rkyv::with` wrapper that serializes `rgb::RGB8` as `[u8; 3]`.
pub struct RkyvRgb8;

impl rkyv::with::ArchiveWith<RGB8> for RkyvRgb8 {
    type Archived = [u8; 3];
    type Resolver = [(); 3];

    fn resolve_with(field: &RGB8, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        let arr = [field.r, field.g, field.b];
        rkyv::Archive::resolve(&arr, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + ?Sized> rkyv::with::SerializeWith<RGB8, S> for RkyvRgb8 {
    fn serialize_with(field: &RGB8, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let arr = [field.r, field.g, field.b];
        rkyv::Serialize::serialize(&arr, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized> rkyv::with::DeserializeWith<[u8; 3], RGB8, D>
    for RkyvRgb8
{
    fn deserialize_with(field: &[u8; 3], _deserializer: &mut D) -> Result<RGB8, D::Error> {
        Ok(RGB8::new(field[0], field[1], field[2]))
    }
}

/// `rkyv::with` wrapper that serializes [`chrono_tz::Tz`] via its IANA timezone name string.
pub struct RkyvTz;

impl rkyv::with::ArchiveWith<Tz> for RkyvTz {
    type Archived = rkyv::string::ArchivedString;
    type Resolver = rkyv::string::StringResolver;

    fn resolve_with(field: &Tz, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        rkyv::string::ArchivedString::resolve_from_str(field.name(), resolver, out);
    }
}

impl<S> rkyv::with::SerializeWith<Tz, S> for RkyvTz
where
    S: rkyv::rancor::Fallible + rkyv::ser::Allocator + rkyv::ser::Writer + ?Sized,
    S::Error: rkyv::rancor::Source,
{
    fn serialize_with(field: &Tz, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        rkyv::string::ArchivedString::serialize_from_str(field.name(), serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized>
    rkyv::with::DeserializeWith<rkyv::string::ArchivedString, Tz, D> for RkyvTz
{
    fn deserialize_with(
        field: &rkyv::string::ArchivedString,
        _deserializer: &mut D,
    ) -> Result<Tz, D::Error> {
        Ok(field.as_str().parse::<Tz>().unwrap_or(Tz::UTC))
    }
}

/// `rkyv::with` wrapper that serializes `Option<chrono_tz::Tz>` via its IANA timezone name string.
pub struct RkyvOptionTz;

impl rkyv::with::ArchiveWith<Option<Tz>> for RkyvOptionTz {
    type Archived = rkyv::option::ArchivedOption<rkyv::string::ArchivedString>;
    type Resolver = Option<rkyv::string::StringResolver>;

    fn resolve_with(
        field: &Option<Tz>,
        resolver: Self::Resolver,
        out: rkyv::Place<Self::Archived>,
    ) {
        let name: Option<String> = field.map(|t| t.name().to_owned());
        rkyv::Archive::resolve(&name, resolver, out);
    }
}

impl<S> rkyv::with::SerializeWith<Option<Tz>, S> for RkyvOptionTz
where
    S: rkyv::rancor::Fallible + rkyv::ser::Allocator + rkyv::ser::Writer + ?Sized,
    S::Error: rkyv::rancor::Source,
{
    fn serialize_with(field: &Option<Tz>, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let name: Option<String> = field.map(|t| t.name().to_owned());
        rkyv::Serialize::serialize(&name, serializer)
    }
}

impl<D: rkyv::rancor::Fallible + ?Sized>
    rkyv::with::DeserializeWith<
        rkyv::option::ArchivedOption<rkyv::string::ArchivedString>,
        Option<Tz>,
        D,
    > for RkyvOptionTz
{
    fn deserialize_with(
        field: &rkyv::option::ArchivedOption<rkyv::string::ArchivedString>,
        _deserializer: &mut D,
    ) -> Result<Option<Tz>, D::Error> {
        Ok(field
            .as_ref()
            .and_then(|s| s.as_str().trim().parse::<Tz>().ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct RkyvPrimitiveSample {
        #[rkyv(with = RkyvNaiveDate)]
        date: NaiveDate,
        #[rkyv(with = RkyvNaiveDateSet)]
        date_set: HashSet<NaiveDate>,
        #[rkyv(with = RkyvRgb8)]
        color: RGB8,
        #[rkyv(with = RkyvTz)]
        tz: Tz,
        #[rkyv(with = RkyvOptionTz)]
        opt_tz_some: Option<Tz>,
        #[rkyv(with = RkyvOptionTz)]
        opt_tz_none: Option<Tz>,
    }

    #[test]
    fn test_rkyv_primitive_wrappers_roundtrip() {
        let mut date_set = HashSet::new();
        date_set.insert(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        date_set.insert(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());

        let sample = RkyvPrimitiveSample {
            date: NaiveDate::from_ymd_opt(2026, 11, 4).unwrap(),
            date_set,
            color: RGB8::new(18, 52, 86),
            tz: Tz::America__Denver,
            opt_tz_some: Some(Tz::Europe__Paris),
            opt_tz_none: None,
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&sample).unwrap();
        let restored: RkyvPrimitiveSample =
            rkyv::from_bytes::<RkyvPrimitiveSample, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(restored, sample);
    }
}
