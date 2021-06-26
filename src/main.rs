use ash::extensions::ext::DebugUtils;
use ash::extensions::khr::{Surface, Swapchain, RayTracingPipeline};
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};
use ash::vk;

use ash_window;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::ControlFlow;

use std::ffi::CString;
use std::iter::FromIterator;

const WINDOW_WIDTH: f64 = 820.0;
const WINDOW_HEIGHT: f64 = 640.0;
const APP_NAME: &str = "My second vulkan app";
const MAX_FRAMES_IN_FLIGHT: usize = 2;
fn main() {
    let entry = unsafe { ash::Entry::new() }.unwrap();

    let event_loop = winit::event_loop::EventLoop::new();

    let window = winit::window::WindowBuilder::new()
        .with_title(APP_NAME)
        .with_inner_size(winit::dpi::PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .with_visible(false)
        .build(&event_loop)
        .unwrap();

    // Интерфейс, через который происходит взаимодействие с Vulkan API.
    let instance = {
        let app_name = CString::new(APP_NAME).unwrap();

        let application_create_info = vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(0)
            .engine_name(&app_name)
            .engine_version(0)
            // Буду использовать версию 1.2.168 в надежде опробовать vulkan ray trasing.
            .api_version(vk::make_version(1, 2, 168));

        let extensions_names_raw = {
            // Для создания surface нам необходимо зарегистрировать платформазависемые расширения, их любезно предоставит
            // библиотека ash_window.
            let mut extensions = ash_window::enumerate_required_extensions(&window).unwrap();
            // Для возможности отлавливать сообшения об ошибкфх в Vulkan необходимо зарегистрироват расширение DebugUtils
            extensions.push(DebugUtils::name());
            let extensions_names_raw = extensions
                .iter()
                .map(|ext| ext.as_ptr())
                .collect::<Vec<_>>();
            extensions_names_raw
        };

        let requred_validation_layer_raw_names = [CString::new("VK_LAYER_KHRONOS_validation").unwrap()];

        let enable_layer_names: Vec<*const i8> = requred_validation_layer_raw_names
            .iter()
            .map(|layer_name| layer_name.as_ptr())
            .collect();

        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&application_create_info)
            .enabled_extension_names(&extensions_names_raw)
            .enabled_layer_names(&enable_layer_names);

        unsafe {
            entry
                .create_instance(&instance_create_info, None)
                .expect("Instance creation error")
        }
    };

    // Регистрируем нашу vulkan_debug_utils_callback функцию. Она позволить получать сообщения об ошибках в stdout.
    let (debug_utils_loader, utils_messenger) = {
        let debug_utils_loader = DebugUtils::new(&entry, &instance);

        let messenger_ci = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING|
                //vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE|
                //vk::DebugUtilsMessageSeverityFlagsEXT::INFO|
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
            )
            .pfn_user_callback(Some(vulkan_debug_utils_callback));

        let utils_messenger = unsafe {
            debug_utils_loader
                .create_debug_utils_messenger(&messenger_ci, None)
                .expect("Debug Utils Callback")
        };

        (debug_utils_loader, utils_messenger)
    };

    // Поскольку Vulkan не зависит от платформы, он не может напрямую взаимодействовать с оконной системой самостоятельно.
    // Для создания surface воспользуемся библиотекой ash_window, она создаст для нас платфозмозависемую поверхность которая
    // будет поддерживаться окном, которое мы уже открыли с помощью winit.
    let surface = unsafe { ash_window::create_surface(&entry, &instance, &window, None) }.unwrap();
    let surface_loader = Surface::new(&entry, &instance);

    let (p_device, graphics_family_index, present_family_index) = {
        let p_devices = unsafe {
            instance
                .enumerate_physical_devices()
                .expect("Physical device error")
        };
        dbg!(&p_devices);

        p_devices
            .iter()
            .find_map(|p_device| {
                // Получим информацию о всех семействах очередей, имеющихся в данном физическом устройстве.
                let queue_families =
                    unsafe { instance.get_physical_device_queue_family_properties(*p_device) };
                // Ищем семейство очередей с потдержкой графики.
                let graphics_family_index =
                    queue_families
                        .iter()
                        .enumerate()
                        .find_map(|(index, &info)| {
                            if info.queue_count > 0
                                && info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                            {
                                Some(index as u32)
                            } else {
                                None
                            }
                        });

                // Ищем семейство очередей без потдержки графики но с возможностью выводит изображение на surfase.
                let present_family_index =
                    queue_families
                        .iter()
                        .enumerate()
                        .find_map(|(index, &info)| {
                            let is_present_support = unsafe {
                                surface_loader.get_physical_device_surface_support(
                                    *p_device,
                                    index as u32,
                                    surface,
                                )
                            }
                            .unwrap();

                            if info.queue_count > 0
                                && is_present_support
                                //&& !info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                            {
                                Some(index as u32)
                            } else {
                                None
                            }
                        });

                // Если удаловь найти физическое устройство с потдержкой графики и вфвода игоброжения, то возвращаем его.
                if let (Some(graphics_family_index), Some(present_family_index)) =
                    (graphics_family_index, present_family_index)
                {
                    Some((*p_device, graphics_family_index, present_family_index))
                } else {
                    None
                }
            })
            .expect("No device found with graphics and present support")
    };

    //Имея физическое устройство – можно создать логическое.
    //Именно оно нам и понадобится для дальнейшей работы с объектами, вроде буферов или шейдеров.
    let device = {
        let device_extension_names_raw = {
            let device_extension_names = vec![Swapchain::name()];
            let device_extension_names_raw = device_extension_names
                .iter()
                .map(|name| name.as_ptr())
                .collect::<Vec<_>>();
            device_extension_names_raw
        };

        let queue_family_indexes =
            std::collections::BTreeSet::from_iter([graphics_family_index, present_family_index]);

        let priorities = [1.0];

        let device_queue_create_infos = queue_family_indexes
            .iter()
            .map(|&queue_family_index| {
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(queue_family_index)
                    .queue_priorities(&priorities)
                    .build()
            })
            .collect::<Vec<_>>();

        let features = vk::PhysicalDeviceFeatures::builder()
            .shader_clip_distance(true)
            .fill_mode_non_solid(true);

        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&device_queue_create_infos)
            .enabled_extension_names(&device_extension_names_raw)
            .enabled_features(&features);

        let device = unsafe {
            instance
                .create_device(p_device, &device_create_info, None)
                .expect("Error create device")
        };

        device
    };

    let swapchain_loader = Swapchain::new(&instance, &device);

    let (swapchain, surface_format, surface_resolution) = {
        // Получаем информацию о поверхности нашего окна.
        let surface_capabilities =
            unsafe { surface_loader.get_physical_device_surface_capabilities(p_device, surface) }
                .unwrap();

        // Количество изображений в цепочке обмена.
        let image_count = if surface_capabilities.max_image_count > 0
            && surface_capabilities.min_image_count + 1 > surface_capabilities.max_image_count
        {
            surface_capabilities.max_image_count
        } else {
            surface_capabilities.min_image_count + 1
        };


        // Формат изображения. Важно указать формат, поддерживаемый поверностию нашего окна.
        let surface_format = {
            let formats_support =
                unsafe { surface_loader.get_physical_device_surface_formats(p_device, surface) }
                    .expect("Failed to query for surface formats.");

            formats_support
                .iter()
                .find_map(|format_support| {
                    if format_support.format == vk::Format::B8G8R8A8_SRGB
                        && format_support.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                    {
                        return Some(format_support.clone());
                    }
                    None
                })
                .unwrap_or(formats_support.first().unwrap().clone())
        };

        // Получаем размер поверхности.
        let surface_resolution = surface_capabilities.current_extent;

        let pre_transform = vk::SurfaceTransformFlagsKHR::IDENTITY;

        // Описываем в как будут подаваться наши изображения из очереди на поверхность.
        let present_mode = vk::PresentModeKHR::FIFO;
            // unsafe { surface_loader.get_physical_device_surface_present_modes(p_device, surface) }
            //     .unwrap()
            //     .iter()
            //     .cloned()
            //     .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
            //     .unwrap_or(vk::PresentModeKHR::FIFO);

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface)
            .min_image_count(image_count)
            .image_color_space(surface_format.color_space)
            .image_format(surface_format.format)
            .image_extent(surface_resolution)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_array_layers(1);

        let swapchain = unsafe {
            swapchain_loader
                .create_swapchain(&swapchain_create_info, None)
                .expect("Error create swapchain")
        };

        (swapchain, surface_format, surface_resolution)
    };

    let swapchain_images = unsafe {
        swapchain_loader
            .get_swapchain_images(swapchain)
            .expect("Failed to get Swapchain Images.")
    };

    //Чтобы использовать что-либо VkImage, в том числе в цепочке подкачки, в конвейере рендеринга,
    //мы должны создать VkImageViewобъект. Просмотр изображения - это буквально взгляд в изображение.
    //В нем описывается, как получить доступ к изображению и к какой части изображения получить доступ,
    //например, следует ли рассматривать его как текстуру глубины 2D текстуры без каких-либо уровней mipmapping.
    let swapchain_images_viev = swapchain_images
        .iter()
        .map(|&image| {
            let imageview_create_info = vk::ImageViewCreateInfo::builder()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::IDENTITY,
                    g: vk::ComponentSwizzle::IDENTITY,
                    b: vk::ComponentSwizzle::IDENTITY,
                    a: vk::ComponentSwizzle::IDENTITY,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image(image);
            unsafe {
                device
                    .create_image_view(&imageview_create_info, None)
                    .expect("Failed to create Image View!")
            }
        })
        .collect::<Vec<_>>();

    let render_pass = {
        // Прежде чем мы сможем завершить создание конвейера, нам нужно сообщить Vulkan о прикреплениях фреймбуфера,
        // которые будут использоваться при рендеринге. Нам нужно указать, сколько будет буферов цвета и глубины,
        // сколько сэмплов использовать для каждого из них и как их содержимое должно обрабатываться во время операций рендеринга.
        // Вся эта информация заключена в объект прохода рендеринга

        // В нашем случае у нас будет только одно прикрепление буфера цвета,
        // представленное одним из изображений из цепочки подкачки
        let color_attachment = vk::AttachmentDescription::builder()
            .format(surface_format.format)
            // Прикрепленный цвета должны соответствовать формату цепь изображений свопа,
            // и мы не делаем ничего с мультисэмплинг еще, так что мы будем придерживаться 1 образца.
            .samples(vk::SampleCountFlags::TYPE_1)
            // У нас есть следующие варианты load_op:
            //      vk::AttachmentLoadOp::LOAD:         Сохранить существующее содержимое вложения
            //      vk::AttachmentLoadOp::CLEAR:        Очистить значения до константы в начале
            //      vk::AttachmentLoadOp::DONT_CARE:    Существующее содержимое не определено; мы не заботимся о них
            .load_op(vk::AttachmentLoadOp::CLEAR)
            //Есть только две возможности store_op:
            //      vk::AttachmentStoreOp::STORE:       Обработанное содержимое будет сохранено в памяти и может быть прочитано позже.
            //      vk::AttachmentStoreOp::DONT_CARE:   Содержимое фреймбуфера будет неопределенным после операции рендеринга.
            .store_op(vk::AttachmentStoreOp::STORE)
            // stencil_load_op/ stencil_store Opприменимы к данным трафарета.
            // Наше приложение ничего не делает с буфером трафарета,
            // поэтому результаты загрузки и сохранения не имеют значения.
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            // Текстуры и фреймбуферы в Vulkan представлены VkImageобъектами с определенным форматом пикселей,
            // однако расположение пикселей в памяти может меняться в зависимости от того, что вы пытаетесь сделать с изображением.
            // Вот некоторые из наиболее распространенных макетов:
            //      vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL: Изображения используются как цветные вложения.
            //      vk::ImageLayout::PRESENT_SRC_KHR:          Изображения, которые будут представлены в цепочке обмена
            //      vk::ImageLayout::TRANSFER_DST_OPTIMAL:     Изображения, которые будут использоваться в качестве места назначения для операции копирования из памяти
            //      vk::ImageLayout::UNDEFINED:                Предостережение этого специального значения заключается в том,
            // что не гарантируется сохранение содержимого изображения, но это не имеет значения, поскольку мы собираемся очистить это все равно.
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build();

        // Один проход рендеринга может состоять из нескольких подпроходов.
        // Подпроходы - это последующие операции рендеринга, которые зависят от содержимого кадровых буферов на предыдущих проходах,
        // например, последовательность эффектов постобработки, которые применяются один за другим.
        // Если вы сгруппируете эти операции рендеринга в один проход рендеринга, то Vulkan сможет изменить порядок операций и
        // сохранить полосу пропускания памяти для, возможно, лучшей производительности.
        // Однако для нашего самого первого треугольника мы будем придерживаться одного подпрохода

        let color_attachment_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

        let subpass = vk::SubpassDescription::builder()
            // Vulkan может также поддерживать подпроходы вычислений в будущем, поэтому мы должны четко указать, что это подпроходы графики
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            // На индекс вложения в этом массиве напрямую ссылается фрагментный шейдер
            // с помощью layout(location = 0) out vec4 outColorдирективы!
            //Подпроходом могут быть ссылки на следующие другие типы вложений:
            //      p_input_attachments:        Вложения, считываемые из шейдера.
            //      p_resolve_attachments:      Вложения, используемые для вложений цветов с множественной выборкой
            //      p_depthStencil_attachment:  Приложение для данных глубины и трафарета
            //      p_preserve_attachments:     Вложения, которые не используются этим подпроходом, но для которых необходимо сохранить данные.
            .color_attachments(std::slice::from_ref(&color_attachment_ref));

        let render_pass_attachments = std::slice::from_ref(&color_attachment);

        // Tеперь, когда были описаны вложение и базовый подпроход, ссылающийся на него, мы можем создать сам проход рендеринга.
        let renderpass_create_info = vk::RenderPassCreateInfo::builder()
            .attachments(render_pass_attachments)
            .subpasses(std::slice::from_ref(&subpass));

        unsafe {
            device
                .create_render_pass(&renderpass_create_info, None)
                .expect("Failed to create render pass!")
        }
    };

    let (graphics_pipeline, pipeline_layout) = {
        let vert_shader_module = {
            let vert_shader_code = include_bytes!("spv/vert.spv");

            let shader_module_create_info = vk::ShaderModuleCreateInfo {
                code_size: vert_shader_code.len(),
                p_code: vert_shader_code.as_ptr() as *const u32,
                ..Default::default()
            };

            unsafe {
                device
                    .create_shader_module(&shader_module_create_info, None)
                    .expect("Failed to create Shader Module from vert.spv!")
            }
        };

        let frag_shader_module = {
            let frag_shader_code = include_bytes!("spv/frag.spv");

            let shader_module_create_info = vk::ShaderModuleCreateInfo {
                code_size: frag_shader_code.len(),
                p_code: frag_shader_code.as_ptr() as *const u32,
                ..Default::default()
            };

            unsafe {
                device
                    .create_shader_module(&shader_module_create_info, None)
                    .expect("Failed to create Shader Module from frag.spv!")
            }
        };

        let main_function_name = CString::new("main").unwrap();

        let shader_stages = vec![
            vk::PipelineShaderStageCreateInfo::builder()
                .module(vert_shader_module)
                .name(&main_function_name)
                .stage(vk::ShaderStageFlags::VERTEX)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .module(frag_shader_module)
                .name(&main_function_name)
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .build(),
        ];

        // Структура описывает формат данных вершин , которые будут переданы в вершинный шейдер.
        //
        // Он описывает это примерно двумя способами:
        //
        //   -Привязки: интервал между данными и данные для каждой вершины
        //    или для каждого экземпляра (см. Создание экземпляров ).
        //
        //   -Описание атрибутов: тип атрибутов, переданных вершинному шейдеру,
        //    привязка для их загрузки и смещение.
        //
        // Поскольку мы жестко кодируем данные вершин непосредственно в вершинном шейдере,
        // мы заполним эту структуру, чтобы указать, что на данный момент нет данных вершин для загрузки
        let vertex_input_state_create_info = vk::PipelineVertexInputStateCreateInfo::default();

        // Структура VkPipelineInputAssemblyStateCreateInfoописывает две вещи:
        //   -какая геометрия будет рисоваться из вершин
        //      vk::PrimitiveTopology::POINT_LIST: точки из вершин
        //      vk::PrimitiveTopology::LINE_LIST: линия из каждых 2 вершин без повторного использования
        //      vk::PrimitiveTopology::LINE_STRIP: конечная вершина каждой строки используется как начальная вершина для следующей строки
        //      vk::PrimitiveTopology::TRIANGLE_LIST: треугольник из каждых 3 вершин без повторного использования
        //      vk::PrimitiveTopology::TRIANGLE_STRIP: вторая и третья вершины каждого треугольника используются как первые две вершины следующего треугольника
        //
        //   -должен ли быть включен перезапуск примитивов
        let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        // Область просмотра в основном описывает область фреймбуфера,
        // в которую будет отображаться вывод. Это почти всегда будет (0, 0)к (width, height)
        let viewports = vec![vk::Viewport::builder()
            .width(WINDOW_WIDTH as f32)
            .height(WINDOW_HEIGHT as f32)
            .min_depth(0.0)
            .max_depth(1.1)
            .build()];

        // В то время как видовые экраны определяют преобразование изображения в буфер кадра,
        // прямоугольники-ножницы определяют, в каких областях фактически будут храниться пиксели.
        // Любые пиксели за пределами прямоугольников-ножниц будут отброшены растеризатором.
        // Они действуют как фильтр, а не как преобразование.
        let scissors = vec![vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: surface_resolution,
        }];

        // Теперь этот видовой экран и прямоугольник с ножницами нужно объединить в состояние видового экрана
        // с помощью vk::PipelineViewportStateCreateInfo
        let viewport_state_create_info = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&viewports)
            .scissors(&scissors);

        // Растеризатор берет геометрию, сформированную вершинами из вершинного шейдера,
        // и превращает ее в фрагменты, которые будут раскрашены фрагментным шейдером.
        // Он также выполняет depth testing, face culling и тест ножниц, и его можно настроить для вывода фрагментов,
        // заполняющих целые полигоны или только края (каркасный рендеринг).
        // Все это настраивается с помощью vk::PipelineRasterizationStateCreateInfo.
        let rasterization_statue_create_info = vk::PipelineRasterizationStateCreateInfo::builder()
            // Если depth_clamp_enable(true), то фрагменты, которые находятся за пределами ближней и дальней плоскостей,
            // прижимаются к ним, а не отбрасываются. Это полезно в некоторых особых случаях, например, в картах теней.
            // Для этого требуется включить функцию графического процессора.
            .depth_clamp_enable(false)
            // Если rasterizer_discard_enable(true), то геометрия никогда не проходит через этап растеризации.
            // Это в основном отключает любой вывод в буфер кадра
            .rasterizer_discard_enable(false)
            // polygon_mode oпределяет , как фрагменты генерируются для геометрии. Доступны следующие режимы:
            //     polygvk::PolygonMode::FILL: заполнить область многоугольника фрагментами
            //     polygvk::PolygonMode::LINE: края многоугольника рисуются как линии
            //     polygvk::PolygonMode::POINT: вершины многоугольника рисуются как точки
            // Для использования любого режима, кроме заливки, необходимо включить функцию графического процессора.
            .polygon_mode(vk::PolygonMode::FILL)
            // Элемент line_width прост, он описывает толщину линий по количеству фрагментов.
            // Максимальная поддерживаемая ширина линии зависит от оборудования и любой линии, более толстой,
            // чем 1.0f требуется  включение функции wide_lines графического процессора.
            .line_width(1.0)
            // Вы можете отключить отбраковку, отсечь передние грани, отсечь задние грани или и то, и другое
            .cull_mode(vk::CullModeFlags::BACK)
            // Указываем как определять какие стороны треугольника передние:  по часовой, или против
            .front_face(vk::FrontFace::CLOCKWISE)
            // Растеризатор может изменять значения глубины, добавляя постоянное значение или смещая их в зависимости от наклона фрагмента.
            // Иногда это используется для отображения теней
            .depth_bias_clamp(0.0)
            .depth_bias_constant_factor(0.0)
            .depth_bias_enable(false)
            .depth_bias_slope_factor(0.0);

        // Структура vk::PipelineMultisampleStateCreateInfo настраивает мультисэмплинг, который является одним из способов сглаживания.
        // Он работает, комбинируя результаты фрагментного шейдера для нескольких многоугольников,
        // которые растрируются в один и тот же пиксель.
        // Для его включения необходимо включить функцию графического процессора.
        let multisample_state_create_info = vk::PipelineMultisampleStateCreateInfo::default();

        // Если вы используете буфер глубины и/или трафарета,
        // вам также необходимо настроить тесты глубины и трафарета с использованием vk::StencilOpState
        let stencil_state = vk::StencilOpState::default();

        let depth_state_create_info = vk::PipelineDepthStencilStateCreateInfo::builder()
            .front(stencil_state)
            .back(stencil_state)
            .max_depth_bounds(1.0)
            .min_depth_bounds(0.0);

        // После того, как шейдер фрагмента вернул цвет, его необходимо объединить с цветом,
        // который уже находится в буфере кадра. Это преобразование известно как смешение цветов,
        // и есть два способа сделать это:
        //      Смешайте старое и новое значение, чтобы получить окончательный цвет.
        //      Объедините старое и новое значение с помощью побитовой операции.
        // Есть два типа структур для настройки смешивания цветов.
        // Первая структура vk::PipelineColorBlendAttachmentState конфигурацию для каждого подключенного фреймбуфера,
        // а вторая структура vk::PipelineColorBlendStateCreateInfo содержит глобальные настройки смешивания цветов.
        // В нашем случае у нас только один фреймбуфер
        let color_blend_attachment_states = vec![vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::all())
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build()];

        // Вторая структура ссылается на массив структур для всех кадровых буферов и позволяет вам устанавливать константы смешивания,
        // которые вы можете использовать в качестве коэффициентов смешивания в вышеупомянутых вычислениях.
        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&color_blend_attachment_states)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        let pipeline_layout = {
            // Вы можете использовать uniform значения в шейдерах, которые являются глобальными переменными, аналогичными динамическим переменным состояния,
            // которые можно изменять во время рисования, чтобы изменить поведение ваших шейдеров без необходимости их воссоздания.
            // Обычно они используются для передачи матрицы преобразования в вершинный шейдер или для создания сэмплеров текстуры во фрагментном шейдере.
            // Эти единые значения необходимо указать во время создания конвейера путем создания VkPipelineLayout объекта.
            // Несмотря на то, что мы не будем использовать их до следующей главы, нам все равно необходимо создать пустой макет конвейера.

            let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::default();
            let pipeline_layout = unsafe {
                device
                    .create_pipeline_layout(&pipeline_layout_create_info, None)
                    .expect("Failed to create pipeline layout!")
            };
            pipeline_layout
        };

        let graphic_pipeline_create_infos = [vk::GraphicsPipelineCreateInfo::builder()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_state_create_info)
            .input_assembly_state(&vertex_input_assembly_state_info)
            .viewport_state(&viewport_state_create_info)
            .rasterization_state(&rasterization_statue_create_info)
            .multisample_state(&multisample_state_create_info)
            .depth_stencil_state(&depth_state_create_info)
            .color_blend_state(&color_blend_state)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .build()];

        let graphics_pipelines = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    &graphic_pipeline_create_infos,
                    None,
                )
                .expect("Failed to create Graphics Pipeline!.")
        };

        unsafe {
            device.destroy_shader_module(vert_shader_module, None);
            device.destroy_shader_module(frag_shader_module, None);
        }

        (graphics_pipelines[0], pipeline_layout)
    };

    // Вложения, указанные во время создания прохода рендеринга, связываются путем их обертывания в VkFramebufferобъект.
    // Объект фреймбуфера ссылается на все VkImageViewобъекты, представляющие вложения.
    let framebuffers = swapchain_images_viev
        .iter()
        .map(|image_view| {
            let attachments = std::slice::from_ref(image_view);

            let framebuffer_create_info = vk::FramebufferCreateInfo::builder()
                .render_pass(render_pass)
                .attachments(attachments)
                .width(surface_resolution.width)
                .height(surface_resolution.height)
                .layers(1);

            unsafe {
                device
                    .create_framebuffer(&framebuffer_create_info, None)
                    .expect("Failed to create Framebuffer!")
            }
        })
        .collect::<Vec<_>>();

    // Мы должны создать пул команд, прежде чем мы сможем создавать буферы команд.
    // Пулы команд управляют памятью, которая используется для хранения буферов, и буферы команд выделяются из них.
    let command_pool = {
        let command_pool_create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(graphics_family_index)
            // Есть два возможных флага для пулов команд:
            //
            //      VK_COMMAND_POOL_CREATE_TRANSIENT_BIT:            Подсказка, что командные буферы очень часто
            // перезаписываются новыми командами (может изменить поведение выделения памяти)
            //
            //      VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT: Разрешить перезапись буферов команд по отдельности,
            // без этого флага все они должны быть сброшены вместе
            .flags(vk::CommandPoolCreateFlags::empty());

        unsafe {
            device
                .create_command_pool(&command_pool_create_info, None)
                .expect("Failed to create Command Pool!")
        }
    };

    let command_buffers = {
        // Буферы команд выделяются с помощью allocate_command_buffers функции, которая принимает
        // vk::CommandBufferAllocateInfo структуру в качестве параметра, указывающего пул команд и количество выделяемых буферов
        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .command_buffer_count(framebuffers.len() as u32)
            //В levelопределяет параметр , если выделенные командные буфера являются первичными или вторичными буферами команд.
            //      vk::CommandBufferLevel::PRIMARY:    Может быть отправлен в очередь для выполнения, но не может быть вызван из других буферов команд.
            //      vk::CommandBufferLevel::SECONDARY:  Не может быть отправлено напрямую, но может быть вызвано из первичных командных буферов.
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers = unsafe {
            device
                .allocate_command_buffers(&command_buffer_allocate_info)
                .expect("Failed to allocate Command Buffers!")
        };

        for (i, &command_buffer) in command_buffers.iter().enumerate() {
            // Мы начинаем запись командного буфера с вызова begin_command_buffer небольшой  vk::CommandBufferBeginInfo структурой
            // в качестве аргумента, который указывает некоторые детали использования этого конкретного командного буфера.
            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                // В flagsопределяет параметр , как мы будем использовать буфер команд. Доступны следующие значения:
                //      vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT:        Командный буфер будет перезаписан сразу после его выполнения.
                //      vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE:   Это дополнительный буфер команд, который будет полностью находиться в пределах одного прохода рендеринга.
                //      vk::CommandBufferUsageFlags::SIMULTANEOUS_USE:       Командный буфер можно повторно отправить, пока он уже ожидает выполнения.
                .flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE);

            unsafe {
                device
                    .begin_command_buffer(command_buffer, &command_buffer_begin_info)
                    .expect("Failed to begin recording Command Buffer at beginning!");
            }

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }];

            // Рисование начинается с начала прохода рендеринга с cmd_begin_render_pass.
            // Этап рендеринга настраивается с использованием некоторых параметров в RenderPassBeginInfo.

            let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
                .render_pass(render_pass)
                .framebuffer(framebuffers[i])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: surface_resolution,
                })
                .clear_values(&clear_values);

            unsafe {
                device.cmd_begin_render_pass(
                    command_buffer,
                    &render_pass_begin_info,
                    vk::SubpassContents::INLINE,
                );
                device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    graphics_pipeline,
                );
                device.cmd_draw(command_buffer, 3, 1, 0, 0);

                device.cmd_end_render_pass(command_buffer);

                device
                    .end_command_buffer(command_buffer)
                    .expect("Failed to record Command Buffer at Ending!");
            }
        }

        command_buffers
    };

    let mut sync_objects = SyncObjects::default();

    let semaphore_create_info = vk::SemaphoreCreateInfo::default();

    let fence_create_info = vk::FenceCreateInfo::builder()
        .flags(vk::FenceCreateFlags::SIGNALED)
        .build();

    for _ in 0..=MAX_FRAMES_IN_FLIGHT {
        unsafe {
            let image_available_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .expect("Failed to create Semaphore Object!");
            let render_finished_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .expect("Failed to create Semaphore Object!");
            let inflight_fence = device
                .create_fence(&fence_create_info, None)
                .expect("Failed to create Fence Object!");

            sync_objects
                .image_available_semaphores
                .push(image_available_semaphore);
            sync_objects
                .render_finished_semaphores
                .push(render_finished_semaphore);
            sync_objects.inflight_fences.push(inflight_fence);
        }
    }

    window.set_visible(true);
    //-----------------------------------------------------------------------------------------------------------------
    let graphics_queue = unsafe { device.get_device_queue(graphics_family_index, 0) };
    let present_queue = unsafe { device.get_device_queue(present_family_index, 0) };

    let image_available_semaphores = sync_objects.image_available_semaphores;
    let render_finished_semaphores = sync_objects.render_finished_semaphores;
    let in_flight_fences = sync_objects.inflight_fences;
    let mut current_frame = 0;

    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
            WindowEvent::KeyboardInput { input, .. } => match input {
                KeyboardInput {
                    virtual_keycode,
                    state,
                    ..
                } => match (virtual_keycode, state) {
                    (Some(VirtualKeyCode::Escape), ElementState::Pressed) => {
                        *control_flow = ControlFlow::Exit
                    }
                    _ => {}
                },
            },
            _ => {}
        },
        Event::MainEventsCleared => {
            if !window.inner_size().width == 0 ||  !window.inner_size().height == 0 {
                window.request_redraw();
            }
        },
        Event::RedrawRequested(_window_id) => {
            // Берем из масисва забор текущего фрэйма
            let wait_fences = [in_flight_fences[current_frame]];

            let (image_index, _is_sub_optimal) = unsafe {
                // Ожидаем
                device
                    .wait_for_fences(&wait_fences, true, std::u64::MAX)
                    .expect("Failed to wait for Fence!");

                // Убедились что видеокарта отрисовала нам в текуший фрэйм. Получаем следующее изображение из цепочки обмена
                swapchain_loader
                    .acquire_next_image(
                        swapchain,
                        std::u64::MAX,
                        // Этот semaphore сигналезирует о получении следующего изображения
                        image_available_semaphores[current_frame],
                        vk::Fence::null(),
                    )
                    .expect("Failed to acquire next image.")
            };

            
            let wait_semaphores = [image_available_semaphores[current_frame]];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signal_semaphores = [render_finished_semaphores[current_frame]];

            let submit_infos = [vk::SubmitInfo::builder()
                // Ждем получения изображения из цепочки обменя
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(std::slice::from_ref(&command_buffers[image_index as usize]))
                // Сигналезоровать о выполнении команд
                .signal_semaphores(&signal_semaphores)
                .build()];

            unsafe {
                // Очищаем забор
                device
                    .reset_fences(&wait_fences)
                    .expect("Failed to reset Fence!");

                // Отправляем команды на выполнения в graphics_queue
                device
                    .queue_submit(
                        graphics_queue,
                        &submit_infos,
                        in_flight_fences[current_frame],
                    )
                    .expect("Failed to execute queue submit.");
            };

            let swapchains = std::slice::from_ref(&swapchain);

            let present_info = vk::PresentInfoKHR::builder()
                // Ждем получения изображения из цепочки обмена. Надо это переписать 
                .wait_semaphores(&wait_semaphores)
                .swapchains(&swapchains)
                .image_indices(std::slice::from_ref(&image_index));

            // Показываем изображение с нашим триугольником на экран
            unsafe {
                swapchain_loader
                    .queue_present(present_queue, &present_info)
                    .expect("Failed to execute queue present.");
            }

            current_frame = current_frame + 1;
            if current_frame > MAX_FRAMES_IN_FLIGHT {
                current_frame = 0;
            };
        }
        Event::LoopDestroyed => {
            unsafe {
                device.device_wait_idle().expect("Failed to wait device idle!");
                device.destroy_command_pool(command_pool, None);
                framebuffers
                    .iter()
                    .for_each(|&framebuffer| device.destroy_framebuffer(framebuffer, None));
                device.destroy_pipeline(graphics_pipeline, None);
                device.destroy_pipeline_layout(pipeline_layout, None);
                device.destroy_render_pass(render_pass, None);
                swapchain_images_viev
                    .iter()
                    .for_each(|&image_view| device.destroy_image_view(image_view, None));
                swapchain_loader.destroy_swapchain(swapchain, None);
                device.destroy_device(None);
                surface_loader.destroy_surface(surface, None);
                debug_utils_loader.destroy_debug_utils_messenger(utils_messenger, None);
                instance.destroy_instance(None);
            };
        }
        _ => (),
    });
}

// Отчет об ошибках — мощный инструмент,
// который позволяет получать информацию от слоев,
// используя функцию обратного вызова (callback).
unsafe extern "system" fn vulkan_debug_utils_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let severity = match message_severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => "[Verbose]",
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => "[Warning]",
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => "[Error]",
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => "[Info]",
        _ => "[Unknown]",
    };
    let types = match message_type {
        vk::DebugUtilsMessageTypeFlagsEXT::GENERAL => "[General]",
        vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE => "[Performance]",
        vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION => "[Validation]",
        _ => "[Unknown]",
    };
    let message = std::ffi::CStr::from_ptr((*p_callback_data).p_message);
    println!("[Debug]{}{}{:?}", severity, types, message);

    vk::FALSE
}

#[derive(Default)]
struct SyncObjects {
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    inflight_fences: Vec<vk::Fence>,
}
