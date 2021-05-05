use crate::models::Lock;
use drogue_cloud_service_api::labels::Operation;
use tokio_postgres::types::ToSql;

pub struct SelectBuilder<'a> {
    select: String,
    params: Vec<&'a (dyn ToSql + Sync)>,

    have_where: bool,
    lock: Lock,
}

impl<'a> SelectBuilder<'a> {
    pub fn new<S: Into<String>>(select: S, params: Vec<&'a (dyn ToSql + Sync)>) -> Self {
        Self {
            select: select.into(),
            params,
            have_where: false,
            lock: Lock::None,
        }
    }

    #[inline]
    fn ensure_where_or_and(&mut self) {
        if !self.have_where {
            self.have_where = true;
            self.select.push_str(" WHERE");
        } else {
            self.select.push_str(" AND");
        }
    }

    pub fn lock(mut self, lock: Lock) -> Self {
        self.lock = lock;
        self
    }

    /// Add a name filter.
    pub fn name(mut self, name: &'a Option<&'a str>) -> Self {
        if let Some(name) = name.as_ref() {
            self.ensure_where_or_and();
            self.params.push(name);
            self.select
                .push_str(&format!(" NAME=${}", self.params.len()));
        }
        self
    }

    /// Add a labels filter.
    pub fn labels(mut self, labels: &'a Vec<Operation>) -> Self {
        for op in labels {
            self.ensure_where_or_and();
            match op {
                Operation::Exists(label) => {
                    self.params.push(label);
                    self.select
                        .push_str(&format!(" LABELS ? ${}", self.params.len()));
                }
                Operation::NotExists(label) => {
                    self.params.push(label);
                    self.select
                        .push_str(&format!(" (NOT LABELS ? ${})", self.params.len()));
                }
                Operation::Eq(label, value) => {
                    self.params.push(label);
                    self.params.push(value);
                    self.select.push_str(&format!(
                        " LABELS ->> ${} = ${}",
                        self.params.len() - 1,
                        self.params.len()
                    ));
                }
                Operation::NotEq(label, value) => {
                    self.params.push(label);
                    self.params.push(value);
                    self.select.push_str(&format!(
                        " LABELS ->> ${} <> ${}",
                        self.params.len() - 1,
                        self.params.len()
                    ));
                }
                Operation::In(label, values) => {
                    self.params.push(label);
                    self.params.push(values);
                    self.select.push_str(&format!(
                        " LABELS ->> ${} IN (${})",
                        self.params.len() - 1,
                        self.params.len()
                    ));
                }
                Operation::NotIn(label, values) => {
                    self.params.push(label);
                    self.params.push(values);
                    self.select.push_str(&format!(
                        " LABELS ->> ${} NOT IN (${})",
                        self.params.len() - 1,
                        self.params.len()
                    ));
                }
            }
        }
        self
    }

    pub fn build(self) -> (String, Vec<&'a (dyn ToSql + Sync)>) {
        let mut select = self.select;

        // append after the where
        select.push_str(self.lock.as_ref());

        // return result
        (select, self.params)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use drogue_cloud_service_api::labels::LabelSelector;
    use std::convert::TryInto;
    use std::fmt::Debug;

    #[test]
    fn test_to_sql_1() {
        let builder = SelectBuilder::new("SELECT * FROM TABLE", Vec::new());

        let (sql, params) = builder.build();

        assert_eq!(sql, "SELECT * FROM TABLE");
        assert_eq!(
            params
                .into_iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<String>>(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_to_sql_name() {
        let mut builder = SelectBuilder::new("SELECT * FROM TABLE", Vec::new());

        builder = builder.name(&Some("Foo"));

        let (sql, params) = builder.build();

        assert_eq!(sql, "SELECT * FROM TABLE WHERE NAME=$1");
        assert_eq!(
            params
                .into_iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<String>>(),
            to_debug(&[&"Foo"])
        );
    }

    #[test]
    fn test_to_labels_1() {
        let mut builder = SelectBuilder::new("SELECT * FROM TABLE", Vec::new());

        let selector: LabelSelector = r#"foo,bar"#.try_into().unwrap();
        builder = builder.labels(&selector.0);

        let (sql, params) = builder.build();

        assert_eq!(
            sql,
            r#"SELECT * FROM TABLE WHERE LABELS ? $1 AND LABELS ? $2"#
        );
        assert_eq!(
            params
                .into_iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<String>>(),
            to_debug(&[&"foo", &"bar"])
        );
    }

    #[test]
    fn test_to_labels_2() {
        let mut builder = SelectBuilder::new("SELECT * FROM TABLE", Vec::new());

        let selector: LabelSelector = r#"!foo,bar in (f1, f2, f3), baz!=abc"#.try_into().unwrap();
        builder = builder.labels(&selector.0);

        let (sql, params) = builder.build();

        assert_eq!(
            sql,
            r#"SELECT * FROM TABLE WHERE (NOT LABELS ? $1) AND LABELS ->> $2 IN ($3) AND LABELS ->> $4 <> $5"#
        );
        assert_eq!(
            params
                .into_iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<String>>(),
            to_debug(&[&"foo", &"bar", &["f1", "f2", "f3"], &"baz", &"abc"])
        );
    }

    fn to_debug(list: &[&dyn Debug]) -> Vec<String> {
        list.iter().map(|s| format!("{:?}", s)).collect()
    }
}
