//! USB device discovery and communication

use std::time::Duration;
use log::{info, warn};
use rusb::{Context, DeviceHandle, UsbContext};
use serde::Serialize;

use crate::config::Config;
use crate::error::AccuChekError;
use crate::protocol::*;

/// A blood glucose reading
#[derive(Debug, Serialize)]
pub struct GlucoseReading {
    pub id: usize,
    pub epoch: i64,
    pub timestamp: String,
    #[serde(rename = "mg/dL")]
    pub mg_dl: u16,
    #[serde(rename = "mmol/L")]
    pub mmol_l: f64,
}

/// Represents an Accu-Chek USB device
#[derive(Debug)]
pub struct AccuChekDevice {
    pub vendor_id: u16,
    pub product_id: u16,
    pub vendor: String,
    pub product: String,
    pub bus_number: u8,
    pub device_address: u8,
    pub config_value: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub send_endpoint: u8,
    pub receive_endpoint: u8,
}

impl AccuChekDevice {
    pub fn show(&self, msg: &str) {
        info!(
            "{}:\n\
            \n    bus number:    {}\n\
            \n    dev address:   {}\n\
            \n    cfg value:     {}\n\
            \n    alt setting:   {}\n\
            \n    alt interface number: {}\n\
            \n    vendor:        (0x{:04x}) {}\n\
            \n    product:       (0x{:04x}) {}\n\
            \n    sndEndPnt:     {}\n\
            \n    rcvEndPnt:     {}",
            msg,
            self.bus_number,
            self.device_address,
            self.config_value,
            self.alternate_setting,
            self.interface_number,
            self.vendor_id,
            self.vendor,
            self.product_id,
            self.product,
            self.send_endpoint,
            self.receive_endpoint
        );
    }
}

/// Check if a USB device matches Accu-Chek characteristics
fn check_device<T: UsbContext>(
    device: &rusb::Device<T>,
    config: &Config,
) -> Option<AccuChekDevice> {
    let desc = device.device_descriptor().ok()?;

    // Accu-Chek has one configuration
    if desc.num_configurations() != 1 {
        info!("Not a match, too many configs to be an Accu-Chek");
        return None;
    }

    // Load first config
    let cfg = device.config_descriptor(0).ok()?;

    // Accu-Chek single config has one interface
    if cfg.num_interfaces() != 1 {
        info!("Not a match, too many interfaces to be an Accu-Chek");
        return None;
    }

    // Get first interface
    let interface = cfg.interfaces().next()?;

    // Accu-Chek has one alt setting
    let interface_descriptors: Vec<_> = interface.descriptors().collect();
    if interface_descriptors.len() != 1 {
        info!("Not a match, too many alt settings to be an Accu-Chek");
        return None;
    }

    let alt_setting = &interface_descriptors[0];

    // Accu-Chek has two endpoints
    if alt_setting.num_endpoints() != 2 {
        info!("Not a match, too many endpoints to be an Accu-Chek");
        return None;
    }

    // Look for valid endpoints
    let mut in_endpoint: Option<u8> = None;
    let mut out_endpoint: Option<u8> = None;

    for endpoint in alt_setting.endpoint_descriptors() {
        // Accu-Chek endpoints should have a max packet size of 64
        if endpoint.max_packet_size() == 64 {
            // Device must be bulk transfer type (not interrupt)
            if endpoint.transfer_type() == rusb::TransferType::Bulk {
                match endpoint.direction() {
                    rusb::Direction::In => in_endpoint = Some(endpoint.address()),
                    rusb::Direction::Out => out_endpoint = Some(endpoint.address()),
                }
            }
        }
    }

    // Need both input and output endpoints
    let (in_ep, out_ep) = match (in_endpoint, out_endpoint) {
        (Some(i), Some(o)) => (i, o),
        _ => {
            info!("Not a match, need at least one input and one output endpoint");
            return None;
        }
    };

    // Open device to get strings
    info!("Found a USB device that looks good, checking further by opening it");
    let handle = device.open().ok()?;

    // Get vendor string
    let vendor = handle
        .read_string_descriptor_ascii(desc.manufacturer_string_index()?)
        .unwrap_or_else(|_| "Unknown".to_string());

    // Get product string  
    let product = handle
        .read_string_descriptor_ascii(desc.product_string_index()?)
        .unwrap_or_else(|_| "Unknown".to_string());

    // Check if device is whitelisted
    if !config.is_device_valid(desc.vendor_id(), desc.product_id()) {
        info!(
            "Nope: looks like it, but that's not the one. This device has mfgr={} device={}",
            vendor, product
        );
        return None;
    }

    info!("========> Found a matching USB device");

    Some(AccuChekDevice {
        vendor_id: desc.vendor_id(),
        product_id: desc.product_id(),
        vendor,
        product,
        bus_number: device.bus_number(),
        device_address: device.address(),
        config_value: cfg.number(),
        interface_number: alt_setting.interface_number(),
        alternate_setting: alt_setting.setting_number(),
        send_endpoint: out_ep,
        receive_endpoint: in_ep,
    })
}

/// Communicate with the Accu-Chek device and download data
fn operate_device<T: UsbContext>(
    device: &rusb::Device<T>,
    accu_chek: &AccuChekDevice,
) -> Result<Vec<GlucoseReading>, AccuChekError> {
    // Open device
    let handle = device.open()?;

    // Detach kernel driver if attached (Linux only)
    #[cfg(unix)]
    {
        if handle.kernel_driver_active(accu_chek.interface_number)? {
            handle.detach_kernel_driver(accu_chek.interface_number)?;
        }
    }

    // Set configuration
    handle.set_active_configuration(accu_chek.config_value)?;

    // Claim interface
    handle.claim_interface(accu_chek.interface_number)?;

    // Set alternate setting
    handle.set_alternate_setting(accu_chek.interface_number, accu_chek.alternate_setting)?;

    info!("Using device snd endpoint = {}", accu_chek.send_endpoint);
    info!("Using device rcv endpoint = {}\n", accu_chek.receive_endpoint);

    // Communication state
    let timeout = Duration::from_secs(5);
    #[allow(unused_assignments)]
    let mut invoke_id: u16 = 0;
    let mut phase_index = 1;
    let mut readings: Vec<GlucoseReading> = Vec::new();
    let mut reading_id = 0;

    // Helper: bulk write
    let bulk_out = |handle: &DeviceHandle<T>, msg_name: &str, data: &[u8], phase: &mut i32| -> Result<(), AccuChekError> {
        info!("\nPhase {}: sending message {}", *phase, msg_name);
        hex_dump_with_header(msg_name, data);
        
        let written = handle.write_bulk(accu_chek.send_endpoint, data, timeout)?;
        if written != data.len() {
            return Err(AccuChekError::Communication(format!(
                "Failed to send message {}: wrote {} of {} bytes",
                msg_name, written, data.len()
            )));
        }
        
        info!("Successfully wrote message {}, size={} (0x{:x}):", msg_name, data.len(), data.len());
        *phase += 1;
        Ok(())
    };

    // Helper: bulk read
    let bulk_in = |handle: &DeviceHandle<T>, msg_name: &str, buffer: &mut [u8], phase: &mut i32| -> Result<usize, AccuChekError> {
        info!("\nPhase {}: receiving message {}", *phase, msg_name);
        
        let read = handle.read_bulk(accu_chek.receive_endpoint, buffer, timeout)?;
        
        info!("Successfully read message \"{}\" from device", msg_name);
        hex_dump_with_header(msg_name, &buffer[..read]);
        
        *phase += 1;
        Ok(read)
    };

    // Buffer for communication
    let mut buffer = [0u8; 1024];

    // Phase 1: Initial control transfer
    {
        info!("Phase 1: initial control transfer in");
        let result = handle.read_control(
            rusb::request_type(
                rusb::Direction::In,
                rusb::RequestType::Standard,
                rusb::Recipient::Device,
            ),
            rusb::constants::LIBUSB_REQUEST_GET_STATUS,
            0,
            0,
            &mut buffer[..2],
            timeout,
        )?;
        
        info!("Initial control transfer succeeded");
        hex_dump_with_header("initial control transfer in", &buffer[..result]);
        phase_index += 1;
    }

    // Phase 2: Wait for pairing request
    {
        bulk_in(&handle, "pairing request", &mut buffer[..64], &mut phase_index)?;
    }

    // Phase 3: Send pairing confirmation
    {
        let mut msg = Vec::new();
        write_be16(&mut msg, APDU_TYPE_ASSOCIATION_RESPONSE);  // msg type
        write_be16(&mut msg, 44);                              // length
        write_be16(&mut msg, 0x0003);                          // accepted-unknown-config
        write_be16(&mut msg, 20601);                           // data-proto-id
        write_be16(&mut msg, 38);                              // data-proto-info length
        write_be32(&mut msg, 0x80000002);                      // protocolVersion
        write_be16(&mut msg, 0x8000);                          // encoding-rules = MDER
        write_be32(&mut msg, 0x80000000);                      // nomenclatureVersion
        write_be32(&mut msg, 0);                               // functionalUnits = normal association
        write_be32(&mut msg, 0x80000000);                      // systemType = sys-type-manager
        write_be16(&mut msg, 8);                               // system-id length
        write_be32(&mut msg, 0x12345678);                      // system-id high
        write_be32(&mut msg, 0x00000000);                      // zero
        write_be32(&mut msg, 0x00000000);                      // zero
        write_be32(&mut msg, 0x00000000);                      // zero
        write_be16(&mut msg, 0x0000);                          // zero

        bulk_out(&handle, "pairing confirmation", &msg, &mut phase_index)?;
    }

    // Phase 4: Wait for config info
    let (pm_store_handle, _nb_segs) = {
        let bytes_read = bulk_in(&handle, "config info", &mut buffer, &mut phase_index)?;
        invoke_id = read_be16(&buffer, 6);
        info!("invokeId after phase {} is: {}", phase_index, invoke_id);

        // Parse config info to get pmStore handle
        let (pm_store_ptr, pm_store_count, pm_store_handle) = get_obj(&buffer[..bytes_read], MDC_MOC_VMO_PMSTORE)?;
        info!("Found pmStore of size {}, handle = {}", pm_store_count, pm_store_handle);

        // Get number of segments
        let (nb_seg_ptr, _) = get_attr(pm_store_ptr, pm_store_count, MDC_ATTR_NUM_SEG)?;
        let nb_segs = read_be16(nb_seg_ptr, 0);
        info!("Data is split into {} segments", nb_segs);

        (pm_store_handle, nb_segs)
    };

    // Phase 5: Send config confirmation
    {
        let mut msg = Vec::new();
        write_be16(&mut msg, APDU_TYPE_PRESENTATION_APDU);
        write_be16(&mut msg, 22);                                        // length
        write_be16(&mut msg, 20);                                        // octet string length
        write_be16(&mut msg, invoke_id);
        write_be16(&mut msg, DATA_APDU_RESPONSE_CONFIRMED_EVENT_REPORT);
        write_be16(&mut msg, 14);                                        // length
        write_be16(&mut msg, 0);                                         // obj-handle = 0
        write_be32(&mut msg, 0);                                         // currentTime = 0
        write_be16(&mut msg, EVENT_TYPE_MDC_NOTI_CONFIG);
        write_be16(&mut msg, 4);                                         // length
        write_be16(&mut msg, 0x4000);                                    // config-report-id
        write_be16(&mut msg, 0);                                         // config-result = accepted-config

        bulk_out(&handle, "config received confirmation", &msg, &mut phase_index)?;
    }

    // Phase 6: Send MDS attribute request
    {
        let mut msg = Vec::new();
        write_be16(&mut msg, APDU_TYPE_PRESENTATION_APDU);
        write_be16(&mut msg, 14);                 // length
        write_be16(&mut msg, 12);                 // octet string length
        write_be16(&mut msg, invoke_id + 1);
        write_be16(&mut msg, DATA_APDU_INVOKE_GET);
        write_be16(&mut msg, 6);                  // length
        write_be16(&mut msg, 0);                  // obj-handle = 0
        write_be32(&mut msg, 0);                  // currentTime = 0

        bulk_out(&handle, "MDS attribute request", &msg, &mut phase_index)?;
    }

    // Phase 7: Read MDS attr answer
    {
        let bytes_read = bulk_in(&handle, "MDS attribute answer", &mut buffer, &mut phase_index)?;
        invoke_id = read_be16(&buffer, 6);
        info!("invokeId after phase {} is: {}", phase_index, invoke_id);

        // Check for abort
        let ret_code = read_be16(&buffer, 0);
        if ret_code == APDU_TYPE_ASSOCIATION_ABORT {
            return Err(AccuChekError::AssociationAborted);
        }
        let _ = bytes_read; // silence warning
    }

    // Phase 8: Send action request
    {
        let mut msg = Vec::new();
        write_be16(&mut msg, APDU_TYPE_PRESENTATION_APDU);
        write_be16(&mut msg, 20);                          // length
        write_be16(&mut msg, 18);                          // octet string length
        write_be16(&mut msg, invoke_id + 1);
        write_be16(&mut msg, DATA_APDU_INVOKE_CONFIRMED_ACTION);
        write_be16(&mut msg, 12);                          // length
        write_be16(&mut msg, pm_store_handle);
        write_be16(&mut msg, ACTION_TYPE_MDC_ACT_SEG_GET_INFO);
        write_be16(&mut msg, 6);                           // length
        write_be16(&mut msg, 1);                           // all segments
        write_be16(&mut msg, 2);                           // length
        write_be16(&mut msg, 0);                           // something

        bulk_out(&handle, "action request", &msg, &mut phase_index)?;
    }

    // Phase 9: Read action request response
    {
        bulk_in(&handle, "action request response", &mut buffer, &mut phase_index)?;
        invoke_id = read_be16(&buffer, 6);
        info!("invokeId after phase {} is: {}", phase_index, invoke_id);
    }

    // Phase 10: Request data segments
    {
        let mut msg = Vec::new();
        write_be16(&mut msg, APDU_TYPE_PRESENTATION_APDU);
        write_be16(&mut msg, 16);                          // length
        write_be16(&mut msg, 14);                          // octet string length
        write_be16(&mut msg, invoke_id + 1);
        write_be16(&mut msg, DATA_APDU_INVOKE_CONFIRMED_ACTION);
        write_be16(&mut msg, 8);                           // length
        write_be16(&mut msg, pm_store_handle);
        write_be16(&mut msg, ACTION_TYPE_MDC_ACT_SEG_TRIG_XFER);
        write_be16(&mut msg, 2);                           // length
        write_be16(&mut msg, 0);                           // segment

        bulk_out(&handle, "request segments", &msg, &mut phase_index)?;
    }

    // Phase 11: Read segment stream header
    {
        let bytes_read = bulk_in(&handle, "segment headers", &mut buffer, &mut phase_index)?;
        invoke_id = read_be16(&buffer, 6);
        info!("invokeId after phase {} is: {}", phase_index, invoke_id);

        // Check for empty data or error
        if bytes_read >= 22 {
            let data_response = read_be16(&buffer, 20);
            if bytes_read == 22 && data_response != 0 {
                if data_response == 3 {
                    warn!("Empty data segment");
                    return Err(AccuChekError::EmptyDataSegment);
                } else {
                    warn!("Error retrieving data, code = {}", data_response);
                    return Err(AccuChekError::Protocol(format!("Data error code: {}", data_response)));
                }
            }
        }

        if bytes_read >= 16 {
            let header_value = read_be16(&buffer, 14);
            if bytes_read < 22 || header_value != ACTION_TYPE_MDC_ACT_SEG_TRIG_XFER {
                return Err(AccuChekError::UnexpectedResponse);
            }
        }
    }

    // Phase 12+: Read data segments
    loop {
        let bytes_read = bulk_in(&handle, "data segment", &mut buffer, &mut phase_index)?;
        let status = buffer[32];
        invoke_id = read_be16(&buffer, 6);
        info!("invokeId after phase {} is: {}", phase_index, invoke_id);

        // Get values needed for ACK
        let u0 = read_be32(&buffer, 22);
        let u1 = read_be32(&buffer, 26);
        let u2 = read_be16(&buffer, 30);

        // Parse samples from segment
        parse_data(&buffer[..bytes_read], &mut readings, &mut reading_id);

        // Send ACK
        {
            let mut msg = Vec::new();
            write_be16(&mut msg, APDU_TYPE_PRESENTATION_APDU);
            write_be16(&mut msg, 30);                          // length
            write_be16(&mut msg, 28);                          // octet string length
            write_be16(&mut msg, invoke_id);
            write_be16(&mut msg, DATA_APDU_RESPONSE_CONFIRMED_EVENT_REPORT);
            write_be16(&mut msg, 22);                          // length
            write_be16(&mut msg, pm_store_handle);
            write_be32(&mut msg, 0xFFFFFFFF);                  // relative time
            write_be16(&mut msg, EVENT_TYPE_MDC_NOTI_SEGMENT_DATA);
            write_be16(&mut msg, 12);
            write_be32(&mut msg, u0);
            write_be32(&mut msg, u1);
            write_be16(&mut msg, u2);
            write_be16(&mut msg, 0x0080);

            bulk_out(&handle, "data segment received ACK", &msg, &mut phase_index)?;
        }

        // Check if this was the last segment
        if (status & 0x40) != 0 {
            break;
        }
    }

    // Disconnect cleanly
    {
        let mut msg = Vec::new();
        write_be16(&mut msg, APDU_TYPE_ASSOCIATION_RELEASE_REQUEST);
        write_be16(&mut msg, 2);
        write_be16(&mut msg, 0x0000);

        bulk_out(&handle, "release request", &msg, &mut phase_index)?;
        bulk_in(&handle, "release confirmation", &mut buffer, &mut phase_index)?;
    }

    info!("Closing USB device");
    Ok(readings)
}

/// Find object of a given class in config buffer
fn get_obj(buffer: &[u8], obj_requested_class: u16) -> Result<(&[u8], u16, u16), AccuChekError> {
    let mut offset = 24;
    let count = read_be16(buffer, offset);
    offset += 2;
    let _dummy = read_be16(buffer, offset);
    offset += 2;

    info!("Got {} objects in config info response", count);

    for _i in 0..count {
        let obj_class = read_be16(buffer, offset);
        offset += 2;
        let obj_handle = read_be16(buffer, offset);
        offset += 2;
        let obj_attr_count = read_be16(buffer, offset);
        offset += 2;
        let obj_size = read_be16(buffer, offset);
        offset += 2;

        if obj_requested_class == obj_class {
            return Ok((&buffer[offset..], obj_attr_count, obj_handle));
        }
        offset += obj_size as usize;
    }

    Err(AccuChekError::Protocol("Object not found in config".to_string()))
}

/// Find attribute of a given class in buffer
fn get_attr(buffer: &[u8], attribute_count: u16, attr_requested_class: u16) -> Result<(&[u8], u16), AccuChekError> {
    info!(
        "Looking for attribute of class {} among {} attributes",
        attr_requested_class, attribute_count
    );

    let mut offset = 0;
    for _i in 0..attribute_count {
        let attr_class = read_be16(buffer, offset);
        offset += 2;
        let attr_size = read_be16(buffer, offset);
        offset += 2;

        if attr_requested_class == attr_class {
            return Ok((&buffer[offset..], attr_size));
        }
        offset += attr_size as usize;
    }

    Err(AccuChekError::Protocol("Attribute not found".to_string()))
}

/// Parse glucose readings from a data segment
fn parse_data(buffer: &[u8], readings: &mut Vec<GlucoseReading>, reading_id: &mut usize) {
    let nb_entries = read_be16(buffer, 30);
    info!("Segment has {} entries", nb_entries);

    let mut offset = 30;

    for _i in 0..nb_entries {
        // Decode BCD-encoded datetime
        let cvt = |x: u8| -> u32 {
            let hi = (x >> 4) & 0x0F;
            let lo = x & 0x0F;
            (hi * 10 + lo) as u32
        };

        let cc = cvt(buffer[offset + 6]);  // century
        let yy = cvt(buffer[offset + 7]);  // year
        let mm = cvt(buffer[offset + 8]);  // month
        let dd = cvt(buffer[offset + 9]);  // day
        let hh = cvt(buffer[offset + 10]); // hour
        let mn = cvt(buffer[offset + 11]); // minute

        // Load value and status
        let vv = read_be16(buffer, offset + 14);
        let ss = read_be16(buffer, offset + 16);
        offset += 12;

        let mg_dl = vv;
        let mmol_l = mg_dl as f64 / 18.0;

        info!(
            "Sample: {:02}{:02}/{:02}/{:02} {:02}:{:02} => (mg/dL={}, mmol/L={:.3}, status=0x{:02x})",
            cc, yy, mm, dd, hh, mn, mg_dl, mmol_l, ss
        );

        // Only add valid readings (status == 0)
        if ss == 0 {
            let year = (cc * 100 + yy) as i32;
            let timestamp = format!(
                "{:04}/{:02}/{:02} {:02}:{:02}",
                year, mm, dd, hh, mn
            );

            // Calculate epoch timestamp
            let epoch = chrono::NaiveDate::from_ymd_opt(year, mm, dd)
                .and_then(|d| d.and_hms_opt(hh, mn, 0))
                .map(|dt| dt.and_utc().timestamp())
                .unwrap_or(0);

            readings.push(GlucoseReading {
                id: *reading_id,
                epoch,
                timestamp,
                mg_dl,
                mmol_l,
            });
            *reading_id += 1;
        }
    }
}

/// Find and operate Accu-Chek devices
pub fn find_and_operate_accuchek(
    context: &Context,
    config: &Config,
    device_index: Option<usize>,
) -> Result<Vec<GlucoseReading>, AccuChekError> {
    // Get list of all USB devices
    info!("Getting list of all USB devices in system from libusb");
    let devices = context.devices()?;
    info!("Found {} USB devices in system", devices.len());

    // Find matching Accu-Chek devices
    let mut valid_devices = Vec::new();
    info!("Searching for valid Accu-Chek devices");

    for device in devices.iter() {
        info!("Checking if device is an Accu-Chek");
        if let Some(accu_chek) = check_device(&device, config) {
            valid_devices.push((device, accu_chek));
        }
    }

    if valid_devices.is_empty() {
        return Err(AccuChekError::NoDeviceFound);
    }

    info!("Found altogether {} Accu-Chek devices", valid_devices.len());

    // Select device
    let selected_index = device_index.unwrap_or(0);
    if selected_index >= valid_devices.len() {
        return Err(AccuChekError::InvalidDeviceIndex(selected_index));
    }

    let (device, accu_chek) = &valid_devices[selected_index];
    accu_chek.show(&format!("Selecting Accu-Chek device #{}:", selected_index));

    // Operate device
    operate_device(device, accu_chek)
}
