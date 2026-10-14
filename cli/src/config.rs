// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::{
    fs::File,
    io::{BufReader, Read},
};

use log::{error, info, warn};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::Value;

use crate::{
    config_arg::{
        VmAddConfigArg, VmAddDtbDeviceConfigArg, VmAddEmulatedDeviceConfigArg, VmAddMemoryRegionConfigArg,
        VmAddPassthroughDeviceIrqsConfigArg, VmAddPassthroughDeviceRegionConfigArg,
        VmAddPassthroughDeviceStreamsIdsConfigArg, VmKernelImageInfo, VmLoadKernelImgFileArg,
        VmMemoryColorBudgetConfigArg, VmSetCpuConfigArg,
    },
    hvc::{
        generate_hvc_mode, HVC_CONFIG, HVC_CONFIG_ADD_VM, HVC_CONFIG_CPU, HVC_CONFIG_DELETE_VM, HVC_CONFIG_DTB_DEVICE,
        HVC_CONFIG_EMULATED_DEVICE, HVC_CONFIG_MEMORY_COLOR_BUDGET, HVC_CONFIG_MEMORY_REGION,
        HVC_CONFIG_PASSTHROUGH_DEVICE_IRQS, HVC_CONFIG_PASSTHROUGH_DEVICE_REGION,
        HVC_CONFIG_PASSTHROUGH_DEVICE_STREAMS_IDS, HVC_CONFIG_UPLOAD_DEVICE_TREE, HVC_CONFIG_UPLOAD_KERNEL_IMAGE,
    },
    dev::{virt_to_phys, HugeBuf, Shyper},
    ioctl_arg::{IOCTL_SYS, IOCTL_SYS_SET_KERNEL_IMG_NAME},
    util::{check_cache_address, file_size, string_to_u64},
};

const CACHE_MAX: usize = 32 * 1024 * 1024;

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum VmType {
    VM_T_LINUX,
    VM_T_BARE_MATEL_APP,
    VM_T_FREERTOS,
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EmuDeviceType {
    EMU_DEVICE_T_CONSOLE,
    EMU_DEVICE_T_GICD,
    EMU_DEVICE_T_GPPT,
    EMU_DEVICE_T_VIRTIO_BLK,
    EMU_DEVICE_T_VIRTIO_NET,
    EMU_DEVICE_T_VIRTIO_CONSOLE,
    EMU_DEVICE_T_SHYPER,
    EMU_DEVICE_T_VIRTIO_BLK_MEDIATED,
    EMU_DEVICE_T_IOMMU,
    EMU_DEVICE_T_SRE,
    EMU_DEVICE_T_SGIR,
    EMU_DEVICE_T_GICR,
    EMU_DEVICE_T_META,
    EMU_DEVICE_T_PLIC,
    EMU_DEVICE_T_APLIC,
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DtbDeviceType {
    DTB_DEVICE_T_CONSOLE,
    DTB_DEVICE_T_GICD,
    DTB_DEVICE_T_GICC,
    DTB_DEVICE_T_GICR,
    DTB_DEVICE_T_PLIC,
    DTB_DEVICE_T_APLIC,
}

// parse hex string to u64, like: "0x8000" -> 32768
fn deserialize_hex_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct HexStringVisitor;

    impl<'de> Visitor<'de> for HexStringVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a hex string")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let value = value.trim();
            if value == "0" {
                Ok(0)
            } else if value.len() <= 2 {
                Err(de::Error::custom("value is not long enough for hex string"))
            } else {
                // Remove the "0x" prefix and parse the remaining string as u64
                u64::from_str_radix(&value[2..], 16).map_err(de::Error::custom)
            }
        }
    }

    deserializer.deserialize_str(HexStringVisitor)
}

fn deserialize_binary_string<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct BinStringVisitor;

    impl<'de> Visitor<'de> for BinStringVisitor {
        type Value = u32;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a binary string, like 0b0111")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Remove the "0b" prefix and parse the remaining string as u64
            u32::from_str_radix(&value[2..], 2).map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_str(BinStringVisitor)
}

// parse colors, like "0-13,14,15,32-63", to num array
fn deserialize_memory_colors_str_to_vec<'de, D>(deserializer: D) -> Result<Option<Vec<u64>>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct MemColorsStrVisitor;

    impl<'de> Visitor<'de> for MemColorsStrVisitor {
        type Value = Option<Vec<u64>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a colors description string, like \"0-13,14,15,32-63\"")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let mut vec: Vec<u64> = Vec::new();
            for color_slice in value.split(',') {
                if !color_slice.contains("-") {
                    let color = color_slice.parse::<u64>().map_err(de::Error::custom)?;
                    vec.push(color);
                    continue;
                }

                let pos = color_slice.find("-").unwrap();
                let len = color_slice.len();
                let start = color_slice[0..pos].parse::<u64>().map_err(de::Error::custom)?;
                let end = color_slice[pos + 1..len].parse::<u64>().map_err(de::Error::custom)?;

                for i in start..end + 1 {
                    vec.push(i);
                }
            }
            Ok(Some(vec))
        }
    }

    deserializer.deserialize_str(MemColorsStrVisitor)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmImageConfig {
    pub kernel_filename: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub kernel_load_ipa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub kernel_entry_point: u64,
    pub device_tree_filename: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub device_tree_load_ipa: u64,
    pub ramdisk_filename: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub ramdisk_load_ipa: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryRegion {
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub ipa_start: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub length: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VmMemoryConfig {
    pub region: Vec<MemoryRegion>,
    // Add default attr, in case when colors field is missing then colors will be filled with None value
    #[serde(deserialize_with = "deserialize_memory_colors_str_to_vec", default)]
    pub colors: Option<Vec<u64>>,
    pub budget: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmCpuConfig {
    pub num: u32,
    #[serde(deserialize_with = "deserialize_binary_string")]
    pub allocate_bitmap: u32,
    pub master: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmEmulatedDeviceConfig {
    pub emulated_device_list: Vec<EmulatedDevice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmulatedDevice {
    pub name: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub base_ipa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub length: u64,
    pub irq_id: usize,
    #[serde(default)]
    pub cfg_num: usize,
    #[serde(deserialize_with = "deserialize_cfg_list", default)]
    pub cfg_list: Vec<u64>,
    pub r#type: EmuDeviceType,
}

fn deserialize_cfg_list<'de, D>(deserializer: D) -> Result<Vec<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    // 解析为serdes_json定义的可变Value，以应对Value是不同值的情况
    let value: Value = Deserialize::deserialize(deserializer)?;
    let mut vec: Vec<u64> = Vec::new();

    match value {
        Value::Array(arr) => {
            for item in arr {
                if let Value::Number(n) = item {
                    if let Some(n) = n.as_u64() {
                        vec.push(n);
                    } else {
                        return Err(de::Error::custom(format!("Can't cast {} to u64", n)));
                    }
                } else if let Value::String(s) = item {
                    match string_to_u64(s) {
                        Ok(n) => vec.push(n),
                        Err(err) => {
                            return Err(de::Error::custom(err));
                        }
                    }
                } else {
                    return Err(de::Error::custom(format!("Not in num/string format: {}", item)));
                }
            }
            Ok(vec)
        }
        _ => Err(de::Error::custom("cfg_list is not array!")),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmPassthroughDeviceConfig {
    pub passthrough_device_list: Vec<PassthroughDevice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PassthroughDevice {
    pub name: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub base_pa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub base_ipa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub length: u64,
    pub smmu_id: Option<usize>,
    pub irq_num: usize,
    pub irq_list: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmDtbDeviceConfig {
    pub dtb_device_list: Vec<DtbDevice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DtbDevice {
    pub name: String,
    pub r#type: DtbDeviceType,
    pub irq_num: usize,
    pub irq_list: Vec<usize>,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub addr_region_ipa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub addr_region_length: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VmConfigEntry {
    pub name: String,
    pub r#type: VmType,
    pub cmdline: String,
    pub image: VmImageConfig,
    pub memory: VmMemoryConfig,
    pub cpu: VmCpuConfig,
    pub emulated_device: VmEmulatedDeviceConfig,
    pub passthrough_device: VmPassthroughDeviceConfig,
    pub dtb_device: VmDtbDeviceConfig,
}

pub fn parse_vm_entry(json_file: String) -> Result<VmConfigEntry, String> {
    // Open the file in read-only mode with buffer.
    let file = File::open(json_file).map_err(|err| err.to_string())?;
    let reader = BufReader::new(file);

    let entry: VmConfigEntry = serde_json::from_reader(reader).map_err(|err| err.to_string())?;
    Ok(entry)
}

pub fn config_delete_vm(vm_id: u64) {
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_DELETE_VM);
    match Shyper::open().and_then(|dev| dev.ioctl_val(fd_event, vm_id)) {
        Ok(()) => info!("DELETE VM [{}] config successfully", vm_id),
        Err(err) => error!("Failed to delete VM[{}] config: {}", vm_id, err),
    }
}

/// Sends one configuration request to the hypervisor.
///
/// The `*ConfigArg` structs are `repr(C)` and the memory their addresses
/// point to stays alive during the call, as the module expects.
fn ioctl_send_config<T>(dev: &Shyper, fd_event: usize, mut arg: T) -> Result<(), String> {
    // SAFETY: see above.
    unsafe { dev.ioctl_ref(fd_event, &mut arg) }.map_err(|err| format!("ioctl failed: {}", err))
}

/// A file name from the configuration, `""` or `"-"` meaning none.
fn config_file(name: &str) -> Option<&str> {
    match name {
        "" | "-" => None,
        name => Some(name),
    }
}

pub fn config_vm_info(vm_cfg: VmConfigEntry, vm_id: u64, dev: &Shyper) -> Result<(), String> {
    // 2. Add VM memory region
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_MEMORY_REGION);
    for region in vm_cfg.memory.region {
        let mem_cfg_arg = VmAddMemoryRegionConfigArg {
            vmid: vm_id,
            ipa_start: region.ipa_start,
            length: region.length,
        };
        ioctl_send_config(dev, fd_event, mem_cfg_arg)
            .map_err(|_| String::from("failed to send vm_add_memory_region_config_arg"))?;
    }

    // 3. Add VM memory color and budget information
    let colors = vm_cfg.memory.colors.as_deref().unwrap_or(&[]);
    let budget = vm_cfg.memory.budget;
    if !colors.is_empty() || budget.is_some() {
        let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_MEMORY_COLOR_BUDGET);
        let cfg = VmMemoryColorBudgetConfigArg {
            vmid: vm_id,
            color_num: colors.len() as u64,
            color_array_addr: if colors.is_empty() { 0 } else { colors.as_ptr() as u64 },
            budget: budget.unwrap_or(0),
        };
        ioctl_send_config(dev, fd_event, cfg)
            .map_err(|_| String::from("failed to send vm_memory_color_budget_config_arg"))?;
    }

    // 4. Set VM CPU config
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_CPU);
    let cpu_cfg_arg = VmSetCpuConfigArg {
        vmid: vm_id,
        num: vm_cfg.cpu.num,
        allocate_bitmap: vm_cfg.cpu.allocate_bitmap,
        master: vm_cfg.cpu.master,
    };
    ioctl_send_config(dev, fd_event, cpu_cfg_arg).map_err(|_| String::from("failed to send vm_cpu_config_arg"))?;

    // 5. Add VM emulated device config
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_EMULATED_DEVICE);
    for device in vm_cfg.emulated_device.emulated_device_list {
        let emu_cfg_arg = VmAddEmulatedDeviceConfigArg {
            vmid: vm_id,
            dev_name_addr: device.name.as_ptr() as u64,
            dev_name_length: device.name.len() as u64,
            base_ipa: device.base_ipa,
            length: device.length,
            irq_id: device.irq_id as u64,
            cfg_list_addr: device.cfg_list.as_ptr() as u64,
            cfg_list_length: device.cfg_list.len() as u64,
            emu_type: device.r#type as u64,
        };
        ioctl_send_config(dev, fd_event, emu_cfg_arg)
            .map_err(|_| String::from("failed to send vm_emulated_device_config_arg"))?;
    }

    // record passthrough device irqs and stream ids
    let mut passthrough_irqs: Vec<u64> = Vec::new();
    let mut passthrough_stream_ids: Vec<u64> = Vec::new();

    // 6. Add VM passthrough device region config.
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_PASSTHROUGH_DEVICE_REGION);
    for device in vm_cfg.passthrough_device.passthrough_device_list {
        let passthrough_cfg_arg = VmAddPassthroughDeviceRegionConfigArg {
            vmid: vm_id,
            base_ipa: device.base_ipa,
            base_pa: device.base_pa,
            length: device.length,
        };
        ioctl_send_config(dev, fd_event, passthrough_cfg_arg)
            .map_err(|_| String::from("failed to send vm_passthrough_device_region_config_arg"))?;

        passthrough_irqs.append(&mut device.irq_list.clone().into_iter().map(|x| x as u64).collect());
        if let Some(smmu_id) = device.smmu_id {
            passthrough_stream_ids.push(smmu_id as u64);
        }
    }

    // 7. Add VM passthrough device irqs.
    if !passthrough_irqs.is_empty() {
        let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_PASSTHROUGH_DEVICE_IRQS);
        let passthrough_irqs_cfg_arg = VmAddPassthroughDeviceIrqsConfigArg {
            vmid: vm_id,
            irqs_addr: passthrough_irqs.as_ptr() as u64,
            irqs_length: passthrough_irqs.len() as u64,
        };
        ioctl_send_config(dev, fd_event, passthrough_irqs_cfg_arg)
            .map_err(|_| String::from("failed to send vm_passthrough_device_irqs_config_arg"))?;
    }

    // 8. Add VM passthrough device streams ids
    if !passthrough_stream_ids.is_empty() {
        let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_PASSTHROUGH_DEVICE_STREAMS_IDS);
        let passthrough_stream_ids_cfg_arg = VmAddPassthroughDeviceStreamsIdsConfigArg {
            vmid: vm_id,
            streams_ids_addr: passthrough_stream_ids.as_ptr() as u64,
            streams_ids_length: passthrough_stream_ids.len() as u64,
        };
        ioctl_send_config(dev, fd_event, passthrough_stream_ids_cfg_arg)
            .map_err(|_| String::from("failed to send vm_passthrough_device_streams_ids_config_arg"))?;
    }

    // 9. Add VM dtb device config
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_DTB_DEVICE);
    for dtb_device in vm_cfg.dtb_device.dtb_device_list {
        let dtb_cfg_arg = VmAddDtbDeviceConfigArg {
            vmid: vm_id,
            dev_name_addr: dtb_device.name.as_ptr() as u64,
            dev_name_length: dtb_device.name.len() as u64,
            dev_type: dtb_device.r#type as u64,
            irq_list_addr: dtb_device.irq_list.as_ptr() as *const u64 as u64,
            irq_list_length: dtb_device.irq_list.len() as u64,
            addr_region_ipa: dtb_device.addr_region_ipa,
            addr_region_length: dtb_device.addr_region_length,
        };
        ioctl_send_config(dev, fd_event, dtb_cfg_arg)
            .map_err(|_| String::from("failed to send vm_dtb_device_config_arg"))?;
    }

    // 10. Copy dtb_file and img_file to memory
    if let Some(dtb) = config_file(&vm_cfg.image.device_tree_filename) {
        copy_device_tree_to_memory(vm_id, dtb.to_string(), dev)
            .map_err(|err| format!("failed to copy device tree {} to memory: {}", dtb, err))?;
    }
    copy_img_file_to_memory(vm_id, vm_cfg.image.kernel_filename.clone(), dev).map_err(|err| {
        format!(
            "failed to copy kernel image {} to memory: {}",
            vm_cfg.image.kernel_filename, err
        )
    })?;

    // 10. Store kernel image file name in kernel module.
    let fd_event = generate_hvc_mode(IOCTL_SYS, IOCTL_SYS_SET_KERNEL_IMG_NAME);
    let mut img_arg = VmKernelImageInfo {
        vm_id,
        image_name: [0; 32],
    };
    let name = vm_cfg.image.kernel_filename.as_bytes();
    if name.len() >= img_arg.image_name.len() {
        return Err(format!(
            "kernel image name {} is too long",
            vm_cfg.image.kernel_filename
        ));
    }
    img_arg.image_name[..name.len()].copy_from_slice(name);
    ioctl_send_config(dev, fd_event, img_arg).map_err(|_| String::from("Failed to set kernel image name"))?;

    Ok(())
}

pub fn config_add_vm(config_json: String) -> Result<u64, String> {
    let vm_cfg = parse_vm_entry(config_json)?;
    let vm_cfg_2 = vm_cfg.clone();
    println!("Parse VM config successfully, VM name [{}]", vm_cfg.name);

    let dev = Shyper::open().map_err(|err| format!("Failed to open /dev/shyper: {}", err))?;

    // 1. Add VM to Hypervisor
    // The hypervisor writes the id of the new VM to `vm_id`.
    let mut vm_id: u64 = 0;
    let vm_add_req = VmAddConfigArg {
        vm_name_addr: vm_cfg.name.as_ptr() as u64,
        vm_name_length: vm_cfg.name.len() as u64,
        vm_type: vm_cfg.r#type as u64,
        cmd_line_addr: vm_cfg.cmdline.as_ptr() as u64,
        cmd_line_length: vm_cfg.cmdline.len() as u64,
        kernel_load_ipa: vm_cfg.image.kernel_load_ipa,
        device_tree_load_ipa: vm_cfg.image.device_tree_load_ipa,
        ramdisk_load_ipa: vm_cfg.image.ramdisk_load_ipa,
        vm_id_addr: &mut vm_id as *mut u64 as u64,
    };
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_ADD_VM);
    ioctl_send_config(&dev, fd_event, vm_add_req).map_err(|_| String::from("config_add_vm failed"))?;
    // SAFETY: `vm_id` was written behind the compiler's back.
    let vm_id = unsafe { std::ptr::read_volatile(&vm_id) };
    info!("Send VM [{}] config successfully", vm_id);

    // deal with failure condition
    match config_vm_info(vm_cfg_2, vm_id, &dev) {
        Ok(_) => {
            info!("Config VM [{}] successfully", vm_id);
            Ok(vm_id)
        }
        Err(err) => {
            error!("Config VM [{}] failed: {}", vm_id, err);
            drop(dev);
            config_delete_vm(vm_id);
            Err(err)
        }
    }
}

/// Copies a file to the memory of VM `vmid` through a huge page buffer, in
/// chunks of at most `CACHE_MAX` bytes.
fn copy_file_to_hypervisor(vmid: u64, filename: String, dev: &Shyper, upload_mode: usize) -> Result<(), String> {
    let file_size = file_size(&filename)?;
    let mut file = File::open(&filename).map_err(|err| format!("open {}: {}", filename, err))?;
    let mut cache = HugeBuf::new(CACHE_MAX).map_err(|err| format!("Allocate cache memory error: {}", err))?;

    // check whether cache address is valid
    if let Err(err) = check_cache_address(cache.as_mut_slice()) {
        warn!("Cache address is invalid");
        return Err(err);
    }
    let cache_ipa = virt_to_phys(cache.addr()).inspect_err(|_| warn!("Failed to get cache pa\n"))?;

    let fd_event = generate_hvc_mode(HVC_CONFIG, upload_mode);
    let mut copied_size: u64 = 0;
    while copied_size < file_size {
        let coping_size = (file_size - copied_size).min(CACHE_MAX as u64);
        file.read_exact(&mut cache.as_mut_slice()[..coping_size as usize])
            .map_err(|err| {
                warn!(
                    "read kernel image {} pos {} size {} failed: {}",
                    filename, copied_size, coping_size, err
                );
                format!("read file {} err", filename)
            })?;

        // Use HVC to copy to Hypervisor memory
        let arg = VmLoadKernelImgFileArg {
            vmid,
            img_size: file_size,
            cache_ipa,
            load_offset: copied_size,
            load_size: coping_size,
        };
        if ioctl_send_config(dev, fd_event, arg).is_err() {
            warn!("Copy_file_to_hypervisor: ioctl failed");
        }
        copied_size += coping_size;
    }
    Ok(())
}

pub fn copy_img_file_to_memory(vmid: u64, filename: String, dev: &Shyper) -> Result<(), String> {
    copy_file_to_hypervisor(vmid, filename, dev, HVC_CONFIG_UPLOAD_KERNEL_IMAGE)
}

pub fn copy_device_tree_to_memory(vmid: u64, filename: String, dev: &Shyper) -> Result<(), String> {
    copy_file_to_hypervisor(vmid, filename, dev, HVC_CONFIG_UPLOAD_DEVICE_TREE)
}
