use std::sync::Arc;

pub use academy_di_derive::Build;
pub use typemap::TypeMap;

mod macros;
mod typemap;

pub trait Provider: Sized {
    fn cache(&mut self) -> &mut TypeMap;
}

#[diagnostic::on_unimplemented(
    message = "The type `{Self}` cannot be built using the provider `{P}`",
    note = "Add `{Self}` to the provider `{P}` or implement `Build` for `{Self}` and make sure \
            all dependencies are satisfied"
)]
pub trait Build<P: Provider>: Clone + 'static {
    fn build(provider: &mut P) -> Self;
}

pub trait Provide: Provider {
    fn provide<T: Build<Self>>(&mut self) -> T {
        T::build(self)
    }
}

impl<P: Provider> Provide for P {}

impl<P, T> Build<P> for Arc<T>
where
    P: Provider,
    T: Build<P>,
{
    fn build(provider: &mut P) -> Self {
        if let Some(cached) = provider.cache().get().cloned() {
            cached
        } else {
            let value = Arc::new(T::build(provider));
            provider.cache().insert(Arc::clone(&value));
            value
        }
    }
}
