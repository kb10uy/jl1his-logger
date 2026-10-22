macro_rules! columns {
    ($($field:ident => $name:literal),* $(,)?) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct Columns {
            $(pub $field: Option<String>,)*
        }

        impl Columns {
            pub const NAMES: &[&str] = &[$($name),*];

            pub fn get(&self, name: &str) -> Option<&str> {
                match name {
                    $($name => self.$field.as_deref(),)*
                    _ => None,
                }
            }

            pub(super) fn slot(&mut self, name: &str) -> Option<&mut Option<String>> {
                match name {
                    $($name => Some(&mut self.$field),)*
                    _ => None,
                }
            }
        }
    };
}

columns! {
    call => "CALL",
    qso_date => "QSO_DATE",
    time_on => "TIME_ON",
    time_off => "TIME_OFF",
    band => "BAND",
    mode => "MODE",
    submode => "SUBMODE",
    freq => "FREQ",
    rst_sent => "RST_SENT",
    rst_rcvd => "RST_RCVD",
    station_callsign => "STATION_CALLSIGN",
    operator => "OPERATOR",
    name => "NAME",
    qth => "QTH",
    gridsquare => "GRIDSQUARE",
    state => "STATE",
    cnty => "CNTY",
    dxcc => "DXCC",
    contest_id => "CONTEST_ID",
    stx_string => "STX_STRING",
    srx_string => "SRX_STRING",
    comment => "COMMENT",
    tx_pwr => "TX_PWR",
    qsl_sent => "QSL_SENT",
    qsl_sent_via => "QSL_SENT_VIA",
    qslsdate => "QSLSDATE",
    qsl_rcvd => "QSL_RCVD",
    qsl_rcvd_via => "QSL_RCVD_VIA",
    qslrdate => "QSLRDATE",
    eqsl_qsl_sent => "EQSL_QSL_SENT",
    eqsl_qslsdate => "EQSL_QSLSDATE",
    eqsl_qsl_rcvd => "EQSL_QSL_RCVD",
    eqsl_qslrdate => "EQSL_QSLRDATE",
    lotw_qsl_sent => "LOTW_QSL_SENT",
    lotw_qslsdate => "LOTW_QSLSDATE",
    lotw_qsl_rcvd => "LOTW_QSL_RCVD",
    lotw_qslrdate => "LOTW_QSLRDATE",
    my_gridsquare => "MY_GRIDSQUARE",
    my_city => "MY_CITY",
    my_state => "MY_STATE",
    my_cnty => "MY_CNTY",
    my_pota_ref => "MY_POTA_REF",
    my_sig => "MY_SIG",
    my_sig_info => "MY_SIG_INFO",
}
