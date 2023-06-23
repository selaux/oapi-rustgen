#[macro_export]
macro_rules! join_ptr {
    ( $x:expr, $( $y:expr ),* ) => {
        {
            let mut p = $x.clone();
            $(
                p.push_back(jsonptr::Token::new($y));
            )*
            p
        }
    };
}
