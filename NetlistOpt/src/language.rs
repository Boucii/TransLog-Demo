use egg::{define_language, Id, Symbol};

define_language! {
    pub enum TransLog {
        "*" = And([Id; 2]),        // AND
        "+" = Or([Id; 2]),         // OR
        "!" = Inv(Id),             // Inverter
        "join" = Join([Id; 2]),    // Join(PUN, PDN)
        "bridge" = Bridge([Id; 5]),// Bridge(a,b,c,d,e)
        "&" = Concat([Id; 2]),     // Concat for multi-output
        "X" = Size([Id; 2]),       // Size operator: (X <device> <scale>)
        Bool(bool),
        Var(Symbol),
    }
}
