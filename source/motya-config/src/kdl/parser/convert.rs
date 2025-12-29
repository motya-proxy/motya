use miette::Result;

pub trait Convert<S>: Sized {
    type Output;
    fn convert(self, state: &S) -> Result<Self::Output>;
}
