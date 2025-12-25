//! IEEE 11073 Personal Health Device Protocol constants and helpers
//!
//! Protocol constants copied from:
//! https://github.com/tidepool-org/uploader/tree/master/lib/drivers/roche
//!
//! These are from the "Continua Health Alliance standard (ISO/IEEE 11073)"
//! See: https://en.wikipedia.org/wiki/Continua_Health_Alliance

#![allow(dead_code)]

// APDU Types
pub const APDU_TYPE_ASSOCIATION_REQUEST: u16 = 0xE200;
pub const APDU_TYPE_ASSOCIATION_RESPONSE: u16 = 0xE300;
pub const APDU_TYPE_ASSOCIATION_RELEASE_REQUEST: u16 = 0xE400;
pub const APDU_TYPE_ASSOCIATION_RELEASE_RESPONSE: u16 = 0xE500;
pub const APDU_TYPE_ASSOCIATION_ABORT: u16 = 0xE600;
pub const APDU_TYPE_PRESENTATION_APDU: u16 = 0xE700;

// Data APDU Types
pub const DATA_APDU_INVOKE_GET: u16 = 0x0103;
pub const DATA_APDU_INVOKE_CONFIRMED_ACTION: u16 = 0x0107;
pub const DATA_APDU_RESPONSE_CONFIRMED_EVENT_REPORT: u16 = 0x0201;
pub const DATA_APDU_RESPONSE_GET: u16 = 0x0203;
pub const DATA_APDU_RESPONSE_CONFIRMED_ACTION: u16 = 0x0207;

// Event Types
pub const EVENT_TYPE_MDC_NOTI_CONFIG: u16 = 0x0D1C;
pub const EVENT_TYPE_MDC_NOTI_SEGMENT_DATA: u16 = 0x0D21;

// Action Types
pub const ACTION_TYPE_MDC_ACT_SEG_GET_INFO: u16 = 0x0C0D;
pub const ACTION_TYPE_MDC_ACT_SEG_GET_ID_LIST: u16 = 0x0C1E;
pub const ACTION_TYPE_MDC_ACT_SEG_TRIG_XFER: u16 = 0x0C1C;
pub const ACTION_TYPE_MDC_ACT_SEG_SET_TIME: u16 = 0x0C17;

// MDC Constants
pub const MDC_MOC_VMO_METRIC: u16 = 4;
pub const MDC_MOC_VMO_METRIC_ENUM: u16 = 5;
pub const MDC_MOC_VMO_METRIC_NU: u16 = 6;
pub const MDC_MOC_VMO_METRIC_SA_RT: u16 = 9;
pub const MDC_MOC_SCAN: u16 = 16;
pub const MDC_MOC_SCAN_CFG: u16 = 17;
pub const MDC_MOC_SCAN_CFG_EPI: u16 = 18;
pub const MDC_MOC_SCAN_CFG_PERI: u16 = 19;
pub const MDC_MOC_VMS_MDS_SIMP: u16 = 37;
pub const MDC_MOC_VMO_PMSTORE: u16 = 61;
pub const MDC_MOC_PM_SEGMENT: u16 = 62;
pub const MDC_ATTR_CONFIRM_MODE: u16 = 2323;
pub const MDC_ATTR_CONFIRM_TIMEOUT: u16 = 2324;
pub const MDC_ATTR_TRANSPORT_TIMEOUT: u16 = 2694;
pub const MDC_ATTR_ID_HANDLE: u16 = 2337;
pub const MDC_ATTR_ID_INSTNO: u16 = 2338;
pub const MDC_ATTR_ID_LABEL_STRING: u16 = 2343;
pub const MDC_ATTR_ID_MODEL: u16 = 2344;
pub const MDC_ATTR_ID_PHYSIO: u16 = 2347;
pub const MDC_ATTR_ID_PROD_SPECN: u16 = 2349;
pub const MDC_ATTR_ID_TYPE: u16 = 2351;
pub const MDC_ATTR_METRIC_STORE_CAPAC_CNT: u16 = 2369;
pub const MDC_ATTR_METRIC_STORE_SAMPLE_ALG: u16 = 2371;
pub const MDC_ATTR_METRIC_STORE_USAGE_CNT: u16 = 2372;
pub const MDC_ATTR_MSMT_STAT: u16 = 2375;
pub const MDC_ATTR_NU_ACCUR_MSMT: u16 = 2378;
pub const MDC_ATTR_NU_CMPD_VAL_OBS: u16 = 2379;
pub const MDC_ATTR_NU_VAL_OBS: u16 = 2384;
pub const MDC_ATTR_NUM_SEG: u16 = 2385;
pub const MDC_ATTR_OP_STAT: u16 = 2387;
pub const MDC_ATTR_POWER_STAT: u16 = 2389;
pub const MDC_ATTR_SA_SPECN: u16 = 2413;
pub const MDC_ATTR_SCALE_SPECN_I16: u16 = 2415;
pub const MDC_ATTR_SCALE_SPECN_I32: u16 = 2416;
pub const MDC_ATTR_SCALE_SPECN_I8: u16 = 2417;
pub const MDC_ATTR_SCAN_REP_PD: u16 = 2421;
pub const MDC_ATTR_SEG_USAGE_CNT: u16 = 2427;
pub const MDC_ATTR_SYS_ID: u16 = 2436;
pub const MDC_ATTR_SYS_TYPE: u16 = 2438;
pub const MDC_ATTR_TIME_ABS: u16 = 2439;
pub const MDC_ATTR_TIME_BATT_REMAIN: u16 = 2440;
pub const MDC_ATTR_TIME_END_SEG: u16 = 2442;
pub const MDC_ATTR_TIME_PD_SAMP: u16 = 2445;
pub const MDC_ATTR_TIME_REL: u16 = 2447;
pub const MDC_ATTR_TIME_STAMP_ABS: u16 = 2448;
pub const MDC_ATTR_TIME_STAMP_REL: u16 = 2449;
pub const MDC_ATTR_TIME_START_SEG: u16 = 2450;
pub const MDC_ATTR_TX_WIND: u16 = 2453;
pub const MDC_ATTR_UNIT_CODE: u16 = 2454;
pub const MDC_ATTR_UNIT_LABEL_STRING: u16 = 2457;
pub const MDC_ATTR_VAL_BATT_CHARGE: u16 = 2460;
pub const MDC_ATTR_VAL_ENUM_OBS: u16 = 2462;
pub const MDC_ATTR_TIME_REL_HI_RES: u16 = 2536;
pub const MDC_ATTR_TIME_STAMP_REL_HI_RES: u16 = 2537;
pub const MDC_ATTR_DEV_CONFIG_ID: u16 = 2628;
pub const MDC_ATTR_MDS_TIME_INFO: u16 = 2629;
pub const MDC_ATTR_METRIC_SPEC_SMALL: u16 = 2630;
pub const MDC_ATTR_SOURCE_HANDLE_REF: u16 = 2631;
pub const MDC_ATTR_SIMP_SA_OBS_VAL: u16 = 2632;
pub const MDC_ATTR_ENUM_OBS_VAL_SIMP_OID: u16 = 2633;
pub const MDC_ATTR_ENUM_OBS_VAL_SIMP_STR: u16 = 2634;
pub const MDC_REG_CERT_DATA_LIST: u16 = 2635;
pub const MDC_ATTR_NU_VAL_OBS_BASIC: u16 = 2636;
pub const MDC_ATTR_PM_STORE_CAPAB: u16 = 2637;
pub const MDC_ATTR_PM_SEG_MAP: u16 = 2638;
pub const MDC_ATTR_PM_SEG_PERSON_ID: u16 = 2639;
pub const MDC_ATTR_SEG_STATS: u16 = 2640;
pub const MDC_ATTR_SEG_FIXED_DATA: u16 = 2641;
pub const MDC_ATTR_SCAN_HANDLE_ATTR_VAL_MAP: u16 = 2643;
pub const MDC_ATTR_SCAN_REP_PD_MIN: u16 = 2644;
pub const MDC_ATTR_ATTRIBUTE_VAL_MAP: u16 = 2645;
pub const MDC_ATTR_NU_VAL_OBS_SIMP: u16 = 2646;
pub const MDC_ATTR_PM_STORE_LABEL_STRING: u16 = 2647;
pub const MDC_ATTR_PM_SEG_LABEL_STRING: u16 = 2648;
pub const MDC_ATTR_TIME_PD_MSMT_ACTIVE: u16 = 2649;
pub const MDC_ATTR_SYS_TYPE_SPEC_LIST: u16 = 2650;
pub const MDC_ATTR_METRIC_ID_PART: u16 = 2655;
pub const MDC_ATTR_ENUM_OBS_VAL_PART: u16 = 2656;
pub const MDC_ATTR_SUPPLEMENTAL_TYPES: u16 = 2657;
pub const MDC_ATTR_TIME_ABS_ADJUST: u16 = 2658;
pub const MDC_ATTR_CLEAR_TIMEOUT: u16 = 2659;
pub const MDC_ATTR_TRANSFER_TIMEOUT: u16 = 2660;
pub const MDC_ATTR_ENUM_OBS_VAL_SIMP_BIT_STR: u16 = 2661;
pub const MDC_ATTR_ENUM_OBS_VAL_BASIC_BIT_STR: u16 = 2662;
pub const MDC_ATTR_METRIC_STRUCT_SMALL: u16 = 2675;
pub const MDC_ATTR_NU_CMPD_VAL_OBS_SIMP: u16 = 2676;
pub const MDC_ATTR_NU_CMPD_VAL_OBS_BASIC: u16 = 2677;
pub const MDC_ATTR_ID_PHYSIO_LIST: u16 = 2678;
pub const MDC_ATTR_SCAN_HANDLE_LIST: u16 = 2679;
pub const MDC_ATTR_TIME_BO: u16 = 2689;
pub const MDC_ATTR_TIME_STAMP_BO: u16 = 2690;
pub const MDC_ATTR_TIME_START_SEG_BO: u16 = 2691;
pub const MDC_ATTR_TIME_END_SEG_BO: u16 = 2692;

/// Get the name of a MDC constant by its value
pub fn find_mdc_name(value: u16) -> Option<&'static str> {
    match value {
        4 => Some("MDC_MOC_VMO_METRIC"),
        5 => Some("MDC_MOC_VMO_METRIC_ENUM"),
        6 => Some("MDC_MOC_VMO_METRIC_NU"),
        9 => Some("MDC_MOC_VMO_METRIC_SA_RT"),
        16 => Some("MDC_MOC_SCAN"),
        17 => Some("MDC_MOC_SCAN_CFG"),
        18 => Some("MDC_MOC_SCAN_CFG_EPI"),
        19 => Some("MDC_MOC_SCAN_CFG_PERI"),
        37 => Some("MDC_MOC_VMS_MDS_SIMP"),
        61 => Some("MDC_MOC_VMO_PMSTORE"),
        62 => Some("MDC_MOC_PM_SEGMENT"),
        2385 => Some("MDC_ATTR_NUM_SEG"),
        _ => None,
    }
}

/// Write a big-endian u16 to a buffer
pub fn write_be16(buffer: &mut Vec<u8>, value: u16) {
    buffer.push((value >> 8) as u8);
    buffer.push((value & 0xFF) as u8);
}

/// Write a big-endian u32 to a buffer
pub fn write_be32(buffer: &mut Vec<u8>, value: u32) {
    buffer.push((value >> 24) as u8);
    buffer.push((value >> 16) as u8);
    buffer.push((value >> 8) as u8);
    buffer.push((value & 0xFF) as u8);
}

/// Read a big-endian u16 from a buffer at offset
pub fn read_be16(buffer: &[u8], offset: usize) -> u16 {
    let hi = buffer[offset] as u16;
    let lo = buffer[offset + 1] as u16;
    (hi << 8) | lo
}

/// Read a big-endian u32 from a buffer at offset
pub fn read_be32(buffer: &[u8], offset: usize) -> u32 {
    let p0 = buffer[offset] as u32;
    let p1 = buffer[offset + 1] as u32;
    let p2 = buffer[offset + 2] as u32;
    let p3 = buffer[offset + 3] as u32;
    (p0 << 24) | (p1 << 16) | (p2 << 8) | p3
}

/// Hex dump a buffer for debugging
pub fn hex_dump(buffer: &[u8]) {
    use log::info;
    
    let mut i = 0;
    while i < buffer.len() {
        let end = std::cmp::min(i + 16, buffer.len());
        
        // Build hex bytes string
        let mut hex_part = String::new();
        for j in i..i + 16 {
            if j < buffer.len() {
                hex_part.push_str(&format!("{:02X} ", buffer[j]));
            } else {
                hex_part.push_str("   ");
            }
        }
        
        // Build ASCII representation
        let mut ascii_part = String::new();
        for j in i..end {
            let c = buffer[j];
            if c.is_ascii_graphic() || c == b' ' {
                ascii_part.push(c as char);
            } else {
                ascii_part.push('.');
            }
        }
        
        info!("{}   {}", hex_part, ascii_part);
        i = i + 16;
    }
}

/// Hex dump with header for debugging
pub fn hex_dump_with_header(name: &str, buffer: &[u8]) {
    log::info!(
        "hexdump of buffer:\n\nBUFFER START \"{}\" size={} (0x{:x}) ===============================================",
        name,
        buffer.len(),
        buffer.len()
    );
    hex_dump(buffer);
    log::info!("BUFFER END ============================================================================================\n");
}
