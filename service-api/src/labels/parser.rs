use super::Operation;
use nom::{
    branch::*, bytes::complete::*, character::complete::*, combinator::*, multi::*, sequence::*,
    AsChar, IResult,
};
use std::fmt::Formatter;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParserError {
    details: String,
}

impl std::error::Error for ParserError {}

impl core::fmt::Display for ParserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "ParserError: {}", self.details)
    }
}

#[cfg(feature = "nom")]
pub fn parse_from(value: &str) -> Result<Vec<Operation>, ParserError> {
    let (remain, ops) = parse(value).map_err(|err| ParserError {
        details: err.to_string(),
    })?;

    if !remain.is_empty() {
        Err(ParserError {
            details: format!("Unparsable remaining content: '{}'", remain),
        })
    } else {
        Ok(ops)
    }
}

fn parse(input: &str) -> IResult<&str, Vec<Operation>> {
    separated_list0(tag(","), parse_one)(input)
}

fn parse_one(input: &str) -> IResult<&str, Operation> {
    alt((
        parse_in,
        parse_not_in,
        parse_equals,
        parse_not_equals,
        parse_exists,
        parse_not_exists,
    ))(input)
}

fn parse_exists(input: &str) -> IResult<&str, Operation> {
    map(parse_label, |label| Operation::Exists(label.into()))(input)
}

fn parse_not_exists(input: &str) -> IResult<&str, Operation> {
    map(preceded(tag("!"), parse_label), |label| {
        Operation::NotExists(label.into())
    })(input)
}

fn parse_equals(input: &str) -> IResult<&str, Operation> {
    map(
        tuple((parse_label, space0, tag("="), parse_value)),
        |(label, _, _, value)| Operation::Eq(label.into(), value.into()),
    )(input)
}

fn parse_not_equals(input: &str) -> IResult<&str, Operation> {
    map(
        tuple((parse_label, space0, tag("!="), parse_value)),
        |(label, _, _, value)| Operation::NotEq(label.into(), value.into()),
    )(input)
}

fn parse_in(input: &str) -> IResult<&str, Operation> {
    map(
        tuple((
            parse_label,
            space1,
            tag("in"),
            space0,
            tag("("),
            separated_list0(tag(","), parse_value),
            tag(")"),
        )),
        |(label, _, _, _, _, value, _)| {
            Operation::In(label.into(), value.iter().map(|s| s.to_string()).collect())
        },
    )(input)
}

fn parse_not_in(input: &str) -> IResult<&str, Operation> {
    map(
        tuple((
            parse_label,
            space1,
            tag("notin"),
            space0,
            tag("("),
            separated_list0(tag(","), parse_value),
            tag(")"),
        )),
        |(label, _, _, _, _, value, _)| {
            Operation::NotIn(label.into(), value.iter().map(|s| s.to_string()).collect())
        },
    )(input)
}

fn parse_label(input: &str) -> IResult<&str, &str> {
    preceded(
        space0,
        recognize(tuple((
            opt(tuple((
                preceded(alpha1, many0(satisfy(|c| c.is_alphanum() || c == '.'))),
                tag("/"),
            ))),
            parse_raw_value,
        ))),
    )(input)
}

fn parse_value(input: &str) -> IResult<&str, &str> {
    map(
        tuple((space0, parse_raw_value, space0)),
        |(_, result, _)| result,
    )(input)
}

fn parse_raw_value(input: &str) -> IResult<&str, &str> {
    recognize(preceded(
        alphanumeric1,
        many0(satisfy(|c| {
            c.is_alphanum() || c == '_' || c == '-' || c == '.'
        })),
    ))(input)
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_raw_value() {
        let (rem, value) = parse_raw_value("foo").unwrap();
        assert_eq!(rem, "");
        assert_eq!(value, "foo");
    }

    #[test]
    fn test_parse_0() {
        assert_eq!(parse_from(""), Ok(vec![]));
    }

    #[test]
    fn test_parse_1() {
        assert_eq!(parse_from("foo"), Ok(vec![Operation::Exists("foo".into())]));
        assert_eq!(
            parse_from("foo/bar"),
            Ok(vec![Operation::Exists("foo/bar".into())])
        );
        assert_eq!(
            parse_from("foo.baz/bar.baz"),
            Ok(vec![Operation::Exists("foo.baz/bar.baz".into())])
        );
    }

    #[test]
    fn test_invalid_label() {
        assert!(parse_from("foo/bar/bar").is_err(),);
        assert!(parse_from("foo/").is_err(),);
        assert!(parse_from("/bar").is_err(),);
        assert!(parse_from("foo-bar/baz").is_err(),);
    }

    #[test]
    fn test_parse_2() {
        assert_eq!(
            parse_from("foo,!bar"),
            Ok(vec![
                Operation::Exists("foo".into()),
                Operation::NotExists("bar".into())
            ])
        );
    }

    #[test]
    fn test_parse_eq_3() {
        assert_eq!(
            parse_from("foo=bar,bar!=baz,foo/bar.baz = baz"),
            Ok(vec![
                Operation::Eq("foo".into(), "bar".into()),
                Operation::NotEq("bar".into(), "baz".into()),
                Operation::Eq("foo/bar.baz".into(), "baz".into()),
            ])
        );
    }

    #[test]
    fn test_parse_in_3() {
        assert_eq!(
            parse_from("foo in (bar, baz),foo notin (baz, bar)"),
            Ok(vec![
                Operation::In("foo".into(), vec!["bar".into(), "baz".into()]),
                Operation::NotIn("foo".into(), vec!["baz".into(), "bar".into()]),
            ])
        );
    }

    #[test]
    fn test_parse_whitespaces_1() {
        assert_eq!(
            parse_from("foo, foo in (bar, baz), foo notin (baz, bar)"),
            Ok(vec![
                Operation::Exists("foo".into()),
                Operation::In("foo".into(), vec!["bar".into(), "baz".into()]),
                Operation::NotIn("foo".into(), vec!["baz".into(), "bar".into()]),
            ])
        );
    }

    #[test]
    fn test_parse_rem() {
        assert_eq!(
            parse_from("foo,#"),
            Err(ParserError {
                details: "Unparsable remaining content: ',#'".into()
            })
        );
    }

    #[test]
    fn test_parse_valid_value_1() {
        assert_eq!(
            parse_from("foo=1"),
            Ok(vec![Operation::Eq("foo".into(), "1".into())])
        );
    }
}
