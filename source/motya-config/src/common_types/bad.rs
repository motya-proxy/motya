use kdl::KdlDocument;
use miette::{Diagnostic, NamedSource, SourceSpan};

#[derive(thiserror::Error, Debug, Diagnostic)]
#[error("Incorrect configuration contents")]
pub struct Bad {
    #[help]
    pub error: String,

    #[source_code]
    pub src: NamedSource<String>,

    #[label("incorrect")]
    pub err_span: SourceSpan,
}

pub trait OptExtParse {
    type Good;

    fn or_bail(
        self,
        msg: impl Into<String>,
        doc: &KdlDocument,
        span: &SourceSpan,
        source_name: impl AsRef<str>,
    ) -> miette::Result<Self::Good>;
}

impl<T> OptExtParse for Option<T> {
    type Good = T;

    fn or_bail(
        self,
        msg: impl Into<String>,
        doc: &KdlDocument,
        span: &SourceSpan, 
        source_name: impl AsRef<str>,
    ) -> miette::Result<Self::Good> {
        match self {
            Some(t) => Ok(t),
            None => Err(Bad::docspan(msg, doc, span, source_name).into()),
        }
    }
}

impl Bad {
    pub fn docspan(msg: impl Into<String>, doc: &KdlDocument, span: &SourceSpan, source_name: impl AsRef<str>) -> Self {
        Self {
            error: msg.into(),
            src: NamedSource::new(source_name, doc.to_string()),
            err_span: span.to_owned(),
        }
    }
}
