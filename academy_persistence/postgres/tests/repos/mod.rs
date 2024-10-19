use academy_models::pagination::PaginationSlice;

mod coins;
mod mfa;
mod oauth2;
mod session;
mod user;

pub fn make_slice(limit: u64, offset: u64) -> PaginationSlice {
    PaginationSlice {
        limit: limit.try_into().unwrap(),
        offset,
    }
}

pub fn sliced<T>(data: &[T], slice: PaginationSlice) -> &[T] {
    let PaginationSlice { limit, offset } = slice;
    let limit = *limit as usize;
    let offset = offset as usize;

    &data[offset.min(data.len())..(offset + limit).min(data.len())]
}
