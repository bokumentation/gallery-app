use cudarc::driver::{CudaContext, CudaSlice, CudaStream, DevicePtr};
use libloading::Library;
use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;

type Status = u32;
type Handle = *mut c_void;
type JpegState = *mut c_void;
type Decoder = *mut c_void;
type JpegStream = *mut c_void;
type DecodeParams = *mut c_void;

type CreateSimple = unsafe extern "C" fn(*mut Handle) -> Status;
type Destroy = unsafe extern "C" fn(Handle) -> Status;
type GetImageInfo = unsafe extern "C" fn(
    Handle,
    *const u8,
    usize,
    *mut i32,
    *mut i32,
    *mut i32,
    *mut i32,
) -> Status;
type DecoderCreate = unsafe extern "C" fn(Handle, u32, *mut Decoder) -> Status;
type DecoderDestroy = unsafe extern "C" fn(Decoder) -> Status;
type DecoderStateCreate = unsafe extern "C" fn(Handle, Decoder, *mut JpegState) -> Status;
type StateDestroy = unsafe extern "C" fn(JpegState) -> Status;
type StreamCreate = unsafe extern "C" fn(Handle, *mut JpegStream) -> Status;
type StreamDestroy = unsafe extern "C" fn(JpegStream) -> Status;
type StreamParse = unsafe extern "C" fn(Handle, *const u8, usize, i32, i32, JpegStream) -> Status;
type ParamsCreate = unsafe extern "C" fn(Handle, *mut DecodeParams) -> Status;
type ParamsDestroy = unsafe extern "C" fn(DecodeParams) -> Status;
type ParamsOutput = unsafe extern "C" fn(DecodeParams, u32) -> Status;
type ParamsScale = unsafe extern "C" fn(DecodeParams, u32) -> Status;
type JpegStateCreate = unsafe extern "C" fn(Handle, *mut JpegState) -> Status;
type BufferPinnedCreate = unsafe extern "C" fn(Handle, *mut c_void, *mut *mut c_void) -> Status;
type BufferDeviceCreate = unsafe extern "C" fn(Handle, *mut c_void, *mut *mut c_void) -> Status;
type BufferPinnedDestroy = unsafe extern "C" fn(*mut c_void) -> Status;
type BufferDeviceDestroy = unsafe extern "C" fn(*mut c_void) -> Status;
type AttachBuffer = unsafe extern "C" fn(JpegState, *mut c_void) -> Status;
type Decode = unsafe extern "C" fn(
    Handle,
    JpegState,
    *const u8,
    usize,
    u32,
    *mut NvjpegImage,
    *mut c_void,
) -> Status;
type DecodeJpeg = unsafe extern "C" fn(
    Handle,
    Decoder,
    JpegState,
    JpegStream,
    *mut NvjpegImage,
    DecodeParams,
    *mut c_void,
) -> Status;

#[repr(C)]
struct NvjpegImage {
    channel: [*mut u8; 4],
    pitch: [usize; 4],
}

struct Api {
    create_simple: CreateSimple,
    destroy: Destroy,
    get_image_info: GetImageInfo,
    decoder_create: DecoderCreate,
    decoder_destroy: DecoderDestroy,
    decoder_state_create: DecoderStateCreate,
    state_destroy: StateDestroy,
    stream_create: StreamCreate,
    stream_destroy: StreamDestroy,
    stream_parse: StreamParse,
    params_create: ParamsCreate,
    params_destroy: ParamsDestroy,
    params_output: ParamsOutput,
    params_scale: ParamsScale,
    decode_jpeg: DecodeJpeg,
    jpeg_state_create: JpegStateCreate,
    decode: Decode,
    buffer_pinned_create: BufferPinnedCreate,
    buffer_device_create: BufferDeviceCreate,
    buffer_pinned_destroy: BufferPinnedDestroy,
    buffer_device_destroy: BufferDeviceDestroy,
    attach_pinned: AttachBuffer,
    attach_device: AttachBuffer,
    _library: Library,
}

unsafe impl Send for Api {}

impl Api {
    fn load() -> Result<Self, String> {
        let library = load_library()?;
        unsafe {
            let create_simple = *library
                .get::<CreateSimple>(b"nvjpegCreateSimple\0")
                .map_err(|err| format!("nvjpegCreateSimple: {err}"))?;
            let destroy = *library
                .get::<Destroy>(b"nvjpegDestroy\0")
                .map_err(|err| format!("nvjpegDestroy: {err}"))?;
            let get_image_info = *library
                .get::<GetImageInfo>(b"nvjpegGetImageInfo\0")
                .map_err(|err| format!("nvjpegGetImageInfo: {err}"))?;
            let decoder_create = *library
                .get::<DecoderCreate>(b"nvjpegDecoderCreate\0")
                .map_err(|err| format!("nvjpegDecoderCreate: {err}"))?;
            let decoder_destroy = *library
                .get::<DecoderDestroy>(b"nvjpegDecoderDestroy\0")
                .map_err(|err| format!("nvjpegDecoderDestroy: {err}"))?;
            let decoder_state_create = *library
                .get::<DecoderStateCreate>(b"nvjpegDecoderStateCreate\0")
                .map_err(|err| format!("nvjpegDecoderStateCreate: {err}"))?;
            let state_destroy = *library
                .get::<StateDestroy>(b"nvjpegJpegStateDestroy\0")
                .map_err(|err| format!("nvjpegJpegStateDestroy: {err}"))?;
            let stream_create = *library
                .get::<StreamCreate>(b"nvjpegJpegStreamCreate\0")
                .map_err(|err| format!("nvjpegJpegStreamCreate: {err}"))?;
            let stream_destroy = *library
                .get::<StreamDestroy>(b"nvjpegJpegStreamDestroy\0")
                .map_err(|err| format!("nvjpegJpegStreamDestroy: {err}"))?;
            let stream_parse = *library
                .get::<StreamParse>(b"nvjpegJpegStreamParse\0")
                .map_err(|err| format!("nvjpegJpegStreamParse: {err}"))?;
            let params_create = *library
                .get::<ParamsCreate>(b"nvjpegDecodeParamsCreate\0")
                .map_err(|err| format!("nvjpegDecodeParamsCreate: {err}"))?;
            let params_destroy = *library
                .get::<ParamsDestroy>(b"nvjpegDecodeParamsDestroy\0")
                .map_err(|err| format!("nvjpegDecodeParamsDestroy: {err}"))?;
            let params_output = *library
                .get::<ParamsOutput>(b"nvjpegDecodeParamsSetOutputFormat\0")
                .map_err(|err| format!("nvjpegDecodeParamsSetOutputFormat: {err}"))?;
            let params_scale = *library
                .get::<ParamsScale>(b"nvjpegDecodeParamsSetScaleFactor\0")
                .map_err(|err| format!("nvjpegDecodeParamsSetScaleFactor: {err}"))?;
            let decode_jpeg = *library
                .get::<DecodeJpeg>(b"nvjpegDecodeJpeg\0")
                .map_err(|err| format!("nvjpegDecodeJpeg: {err}"))?;
            let jpeg_state_create = *library
                .get::<JpegStateCreate>(b"nvjpegJpegStateCreate\0")
                .map_err(|err| format!("nvjpegJpegStateCreate: {err}"))?;
            let decode = *library
                .get::<Decode>(b"nvjpegDecode\0")
                .map_err(|err| format!("nvjpegDecode: {err}"))?;
            let buffer_pinned_create = *library
                .get::<BufferPinnedCreate>(b"nvjpegBufferPinnedCreate\0")
                .map_err(|err| format!("nvjpegBufferPinnedCreate: {err}"))?;
            let buffer_device_create = *library
                .get::<BufferDeviceCreate>(b"nvjpegBufferDeviceCreate\0")
                .map_err(|err| format!("nvjpegBufferDeviceCreate: {err}"))?;
            let buffer_pinned_destroy = *library
                .get::<BufferPinnedDestroy>(b"nvjpegBufferPinnedDestroy\0")
                .map_err(|err| format!("nvjpegBufferPinnedDestroy: {err}"))?;
            let buffer_device_destroy = *library
                .get::<BufferDeviceDestroy>(b"nvjpegBufferDeviceDestroy\0")
                .map_err(|err| format!("nvjpegBufferDeviceDestroy: {err}"))?;
            let attach_pinned = *library
                .get::<AttachBuffer>(b"nvjpegStateAttachPinnedBuffer\0")
                .map_err(|err| format!("nvjpegStateAttachPinnedBuffer: {err}"))?;
            let attach_device = *library
                .get::<AttachBuffer>(b"nvjpegStateAttachDeviceBuffer\0")
                .map_err(|err| format!("nvjpegStateAttachDeviceBuffer: {err}"))?;

            Ok(Self {
                create_simple,
                destroy,
                get_image_info,
                decoder_create,
                decoder_destroy,
                decoder_state_create,
                state_destroy,
                stream_create,
                stream_destroy,
                stream_parse,
                params_create,
                params_destroy,
                params_output,
                params_scale,
                decode_jpeg,
                jpeg_state_create,
                decode,
                buffer_pinned_create,
                buffer_device_create,
                buffer_pinned_destroy,
                buffer_device_destroy,
                attach_pinned,
                attach_device,
                _library: library,
            })
        }
    }
}

fn load_library() -> Result<Library, String> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(path) = std::env::var("NVJPEG_LIBRARY") {
        candidates.push(path);
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join(".local/lib/rust-gallery/libnvjpeg.so.13")
                .to_string_lossy()
                .into_owned(),
        );
    }
    candidates.extend([
        "/usr/local/cuda-13.1/targets/x86_64-linux/lib/libnvjpeg.so.13".to_owned(),
        "/usr/local/cuda/targets/x86_64-linux/lib/libnvjpeg.so.13".to_owned(),
        "/usr/local/cuda/lib64/libnvjpeg.so.13".to_owned(),
        "/usr/lib/x86_64-linux-gnu/libnvjpeg.so.13".to_owned(),
        "libnvjpeg.so.13".to_owned(),
        "libnvjpeg.so.12".to_owned(),
        "libnvjpeg.so".to_owned(),
    ]);

    let mut errors = Vec::new();
    for candidate in candidates {
        match unsafe { Library::new(&candidate) } {
            Ok(library) => {
                log::info!("nvJPEG loaded from {candidate}");
                return Ok(library);
            }
            Err(error) => errors.push(format!("{candidate}: {error}")),
        }
    }
    Err(format!("nvJPEG library not found ({})", errors.join("; ")))
}

#[derive(Clone, Copy, Debug)]
pub struct DecodePlan {
    pub source: [u32; 2],
    pub output: [u32; 2],
    pub scale: u32,
    pub pitch: usize,
}

pub struct NvJpegDecoder {
    api: Api,
    handle: Handle,
    decoder: Decoder,
    state: JpegState,
    legacy_state: JpegState,
    pinned_buffer: *mut c_void,
    device_buffer: *mut c_void,
    stream: JpegStream,
    params: DecodeParams,
    _cuda: Arc<CudaContext>,
    cuda_stream: Arc<CudaStream>,
}

unsafe impl Send for NvJpegDecoder {}

impl NvJpegDecoder {
    pub fn new() -> Result<Self, String> {
        let api = Api::load()?;
        let cuda = CudaContext::new(0).map_err(|err| format!("CUDA init: {err:?}"))?;
        cuda.bind_to_thread()
            .map_err(|err| format!("CUDA bind: {err:?}"))?;
        let cuda_stream = cuda.default_stream();

        let mut handle: Handle = std::ptr::null_mut();
        check(
            unsafe { (api.create_simple)(&mut handle) },
            "nvjpegCreateSimple",
        )?;

        let mut decoder: Decoder = std::ptr::null_mut();
        let mut status = unsafe { (api.decoder_create)(handle, 0, &mut decoder) };
        if status != 0 {
            status = unsafe { (api.decoder_create)(handle, 0, &mut decoder) };
        }
        if status != 0 {
            unsafe { (api.destroy)(handle) };
            return Err(format!("nvjpegDecoderCreate failed: {status}"));
        }

        let mut state: JpegState = std::ptr::null_mut();
        check(
            unsafe { (api.decoder_state_create)(handle, decoder, &mut state) },
            "nvjpegDecoderStateCreate",
        )?;

        let mut legacy_state: JpegState = std::ptr::null_mut();
        check(
            unsafe { (api.jpeg_state_create)(handle, &mut legacy_state) },
            "nvjpegJpegStateCreate",
        )?;

        let mut pinned_buffer: *mut c_void = std::ptr::null_mut();
        check(
            unsafe { (api.buffer_pinned_create)(handle, std::ptr::null_mut(), &mut pinned_buffer) },
            "nvjpegBufferPinnedCreate",
        )?;
        let mut device_buffer: *mut c_void = std::ptr::null_mut();
        check(
            unsafe { (api.buffer_device_create)(handle, std::ptr::null_mut(), &mut device_buffer) },
            "nvjpegBufferDeviceCreate",
        )?;

        check(
            unsafe { (api.attach_pinned)(state, pinned_buffer) },
            "nvjpegStateAttachPinnedBuffer",
        )?;
        check(
            unsafe { (api.attach_device)(state, device_buffer) },
            "nvjpegStateAttachDeviceBuffer",
        )?;

        let mut stream: JpegStream = std::ptr::null_mut();
        check(
            unsafe { (api.stream_create)(handle, &mut stream) },
            "nvjpegJpegStreamCreate",
        )?;

        let mut params: DecodeParams = std::ptr::null_mut();
        check(
            unsafe { (api.params_create)(handle, &mut params) },
            "nvjpegDecodeParamsCreate",
        )?;
        check(
            unsafe { (api.params_output)(params, 5) },
            "nvjpegDecodeParamsSetOutputFormat",
        )?;

        Ok(Self {
            api,
            handle,
            decoder,
            state,
            legacy_state,
            pinned_buffer,
            device_buffer,
            stream,
            params,
            _cuda: cuda,
            cuda_stream,
        })
    }

    pub fn decode_jpeg(
        &mut self,
        data: &[u8],
        target: [u32; 2],
    ) -> Result<(Vec<u8>, [u32; 2], [u32; 2]), String> {
        match self.decode_jpeg_scaled(data, target) {
            Ok(result) => return Ok(result),
            Err(error) => log::warn!("scaled nvJPEG decode failed: {error}"),
        }
        self.decode_jpeg_legacy(data)
    }

    pub fn plan(&mut self, data: &[u8], target: [u32; 2]) -> Result<DecodePlan, String> {
        let mut components = 0i32;
        let mut subsampling = 0i32;
        let mut widths = [0i32; 4];
        let mut heights = [0i32; 4];
        check(
            unsafe {
                (self.api.get_image_info)(
                    self.handle,
                    data.as_ptr(),
                    data.len(),
                    &mut components,
                    &mut subsampling,
                    widths.as_mut_ptr(),
                    heights.as_mut_ptr(),
                )
            },
            "nvjpegGetImageInfo",
        )?;
        let width = widths[0].max(1) as u32;
        let height = heights[0].max(1) as u32;
        let (scale, divisor) = choose_scale(width, height, target);
        let output_width = width.div_ceil(divisor);
        let output_height = height.div_ceil(divisor);
        let pitch = ((output_width as usize * 3 + 255) & !255).max(256);
        Ok(DecodePlan {
            source: [width, height],
            output: [output_width, output_height],
            scale,
            pitch,
        })
    }

    pub fn decode_jpeg_into(
        &mut self,
        data: &[u8],
        plan: DecodePlan,
        pointer: *mut u8,
    ) -> Result<(), String> {
        check(
            unsafe { (self.api.params_scale)(self.params, plan.scale) },
            "nvjpegDecodeParamsSetScaleFactor",
        )?;
        check(
            unsafe {
                (self.api.stream_parse)(self.handle, data.as_ptr(), data.len(), 0, 1, self.stream)
            },
            "nvjpegJpegStreamParse",
        )?;
        let mut image = NvjpegImage {
            channel: [
                pointer,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            pitch: [plan.pitch, 0, 0, 0],
        };
        check(
            unsafe {
                (self.api.decode_jpeg)(
                    self.handle,
                    self.decoder,
                    self.state,
                    self.stream,
                    &mut image,
                    self.params,
                    self.cuda_stream.cu_stream().cast(),
                )
            },
            "nvjpegDecodeJpeg",
        )
    }

    pub fn cuda(&self) -> &Arc<CudaContext> {
        &self._cuda
    }

    pub fn cuda_stream(&self) -> &Arc<CudaStream> {
        &self.cuda_stream
    }

    fn decode_jpeg_scaled(
        &mut self,
        data: &[u8],
        target: [u32; 2],
    ) -> Result<(Vec<u8>, [u32; 2], [u32; 2]), String> {
        let plan = self.plan(data, target)?;
        let length = plan.pitch * plan.output[1] as usize;
        let stream = self.cuda_stream.clone();
        let buffer: CudaSlice<u8> =
            unsafe { stream.alloc::<u8>(length) }.map_err(|err| format!("CUDA alloc: {err:?}"))?;
        let (pointer, _guard) = buffer.device_ptr(&stream);
        self.decode_jpeg_into(data, plan, pointer as *mut u8)?;
        drop(_guard);

        let rgb = stream
            .memcpy_dtov(&buffer)
            .map_err(|err| format!("CUDA download: {err:?}"))?;
        let output_width = plan.output[0];
        let output_height = plan.output[1];
        let pitch = plan.pitch;

        let mut rgba = vec![255u8; output_width as usize * output_height as usize * 4];
        for y in 0..output_height as usize {
            let src_row = &rgb[y * pitch..y * pitch + output_width as usize * 3];
            let dst_row =
                &mut rgba[y * output_width as usize * 4..(y + 1) * output_width as usize * 4];
            for (source, destination) in src_row.chunks_exact(3).zip(dst_row.chunks_exact_mut(4)) {
                destination[0] = source[0];
                destination[1] = source[1];
                destination[2] = source[2];
            }
        }

        Ok((rgba, [output_width, output_height], plan.source))
    }

    fn decode_jpeg_legacy(&mut self, data: &[u8]) -> Result<(Vec<u8>, [u32; 2], [u32; 2]), String> {
        let mut components = 0i32;
        let mut subsampling = 0i32;
        let mut widths = [0i32; 4];
        let mut heights = [0i32; 4];
        check(
            unsafe {
                (self.api.get_image_info)(
                    self.handle,
                    data.as_ptr(),
                    data.len(),
                    &mut components,
                    &mut subsampling,
                    widths.as_mut_ptr(),
                    heights.as_mut_ptr(),
                )
            },
            "nvjpegGetImageInfo",
        )?;
        let width = widths[0].max(1) as u32;
        let height = heights[0].max(1) as u32;
        let pitch = ((width as usize * 3 + 255) & !255).max(256);
        let length = pitch * height as usize;
        let buffer: CudaSlice<u8> = unsafe { self.cuda_stream.alloc::<u8>(length) }
            .map_err(|err| format!("CUDA alloc: {err:?}"))?;
        let (pointer, _guard) = buffer.device_ptr(&self.cuda_stream);
        let mut image = NvjpegImage {
            channel: [
                pointer as *mut u8,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ],
            pitch: [pitch, 0, 0, 0],
        };
        check(
            unsafe {
                (self.api.decode)(
                    self.handle,
                    self.legacy_state,
                    data.as_ptr(),
                    data.len(),
                    5,
                    &mut image,
                    self.cuda_stream.cu_stream().cast(),
                )
            },
            "nvjpegDecode",
        )?;
        let rgb = self
            .cuda_stream
            .memcpy_dtov(&buffer)
            .map_err(|err| format!("CUDA download: {err:?}"))?;
        let mut rgba = vec![255u8; width as usize * height as usize * 4];
        for y in 0..height as usize {
            let src_row = &rgb[y * pitch..y * pitch + width as usize * 3];
            let dst_row = &mut rgba[y * width as usize * 4..(y + 1) * width as usize * 4];
            for (source, destination) in src_row.chunks_exact(3).zip(dst_row.chunks_exact_mut(4)) {
                destination[0] = source[0];
                destination[1] = source[1];
                destination[2] = source[2];
            }
        }
        Ok((rgba, [width, height], [width, height]))
    }
}

impl Drop for NvJpegDecoder {
    fn drop(&mut self) {
        unsafe {
            (self.api.state_destroy)(self.state);
            (self.api.state_destroy)(self.legacy_state);
            (self.api.buffer_pinned_destroy)(self.pinned_buffer);
            (self.api.buffer_device_destroy)(self.device_buffer);
            (self.api.stream_destroy)(self.stream);
            (self.api.params_destroy)(self.params);
            (self.api.decoder_destroy)(self.decoder);
            (self.api.destroy)(self.handle);
        }
    }
}

fn choose_scale(_width: u32, _height: u32, _target: [u32; 2]) -> (u32, u32) {
    // This NVDEC generation rejects scaled JPEG decode (nvJPEG returns
    // INVALID_PARAMETER), so decode at full resolution and let the viewer
    // either sample it directly or convert it with the GPU shader.
    (0, 1)
}

fn check(status: Status, operation: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed with status {status}"))
    }
}

pub fn is_jpeg(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            let extension = extension.to_ascii_lowercase();
            extension == "jpg" || extension == "jpeg"
        })
        .unwrap_or(false)
}
