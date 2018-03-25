#![feature(slice_patterns)]

extern crate num_complex;
extern crate image;
extern crate palette;
extern crate rand;
#[macro_use] extern crate rand_derive;
extern crate pathfinding;
extern crate chrono;
#[macro_use] extern crate structopt;
extern crate fractalz;

use std::str::FromStr;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use num_complex::Complex64;
use image::FilterType;
use image::RgbImage;
use image::imageops;
use palette::Gradient;
use palette::rgb::LinSrgb;
use rand::{SeedableRng, Rng};
use rand::StdRng;
use pathfinding::dijkstra;
use structopt::StructOpt;
use chrono::{Utc, DateTime, Timelike};

use fractalz::Fractal;
use fractalz::{Julia, Mandelbrot};
use fractalz::Camera;
use fractalz::{ComplexPalette, SubGradient};
use fractalz::{produce_image, edges};

fn find_point<P>(start: (u32, u32),
                 image: &RgbImage,
                 predicate: P)
                 -> Option<(u32, u32)>
where
    P: Fn(&image::Rgb<u8>) -> bool
{
    let (width, height) = image.dimensions();

    let result = dijkstra(&start, |&(x, y)| {
        let mut neighbours = Vec::new();
        if x > 0 {
            neighbours.push(((x - 1, y), 1))
        }
        if y > 0 {
            neighbours.push(((x, y - 1), 1))
        }
        if x < width - 1 {
            neighbours.push(((x + 1, y), 1))
        }
        if y < height - 1 {
            neighbours.push(((x, y + 1), 1))
        }
        neighbours
    },
    |&(x, y)| predicate(&image.get_pixel(x, y)));

    result.map(|(path, _)| *path.last().unwrap())
}

fn floor_to_hour(datetime: DateTime<Utc>) -> Option<DateTime<Utc>> {
    datetime
        .with_minute(0)?
        .with_second(0)?
        .with_nanosecond(0)
}

#[derive(Debug, StructOpt)]
struct Settings {
    /// The date to use as a seed,
    /// the default is the current datetime rounded to the hour.
    #[structopt(long = "date-seed")]
    date_seed: Option<DateTime<Utc>>,

    /// Antialiazing used for the images generated (a power of 4).
    #[structopt(long = "antialiazing", default_value = "4")]
    antialiazing: u32,

    /// Screen dimensions used for all image generations.
    #[structopt(long = "screen-dimensions", default_value = "800x600")]
    screen_dimensions: Option<ScreenDimensions>,

    /// Whether the program produce all images while diving in the fractal.
    #[structopt(long = "produce-debug-images", default_value = "true")]
    produce_debug_images: bool,
}

#[derive(Debug, Copy, Clone)]
struct ScreenDimensions(u32, u32);

impl ScreenDimensions {
    fn tuple(&self) -> (u32, u32) {
        let ScreenDimensions(width, height) = *self;
        (width, height)
    }
}

impl FromStr for ScreenDimensions {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        let mut splitted = s.split('x');

        let invalid_msg = "invalid dimension format";

        let width = splitted.next().ok_or(invalid_msg)?;
        let height = splitted.next().ok_or(invalid_msg)?;
        if splitted.next().is_some() {
            return Err(invalid_msg)
        }

        let width = width.parse().map_err(|_| "invalid width")?;
        let height = height.parse().map_err(|_| "invalid height")?;

        Ok(ScreenDimensions(width, height))
    }
}

impl Default for ScreenDimensions {
    fn default() -> Self {
        ScreenDimensions(800, 600)
    }
}

#[derive(Debug, Rand)]
enum FractalType {
    Mandelbrot,
    Julia,
}

fn is_power_of_four(n: u32) -> bool {
    n.count_ones() == 1 && n.trailing_zeros() % 2 == 0
}

/// Find a good target point that will not be a black area:
///   - create a grayscale image
///   - blur the grayscale image
///   - find the nearest black point
///   - create an edge image of the first grayscaled image
///   - find the nearest white point on the edged image starting from the previous black point
fn find_target_point<F, R>(rng: &mut R,
                      fractal: &F,
                      camera: &Camera,
                      dimensions: (u32, u32))
                      -> Option<(u32, u32)>
where
    F: Fractal,
    R: Rng
{
    let (width, height) = dimensions;

    let grayscaled = produce_image(fractal, camera, dimensions, |i| image::Rgb { data: [i; 3] });
    let blurred = imageops::blur(&grayscaled, 10.0);
    let black_point = {
        let start = (rng.gen_range(0, width), rng.gen_range(0, height));
        find_point(start, &blurred, |p| p.data[0] <= 128)
    };

    black_point.and_then(|black_point| {
        let edged = edges(&grayscaled);
        find_point(black_point, &edged, |p| p.data[0] >= 128)
    })
}

fn produce_debug_image<F>(fractal: &F,
                          camera: &Camera,
                          dimensions: (u32, u32),
                          n: usize)
where
    F: Fractal
{
    let grayscaled = produce_image(fractal, camera, dimensions, |i| image::Rgb { data: [i; 3] });
    let image = edges(&grayscaled);
    image.save(format!("./spotted-area-{:03}.png", n)).unwrap();
}

fn main() {
    let settings = Settings::from_args();

    if !is_power_of_four(settings.antialiazing) {
        eprintln!("The specified antialiazing must be a power of four");
        ::std::process::exit(1);
    }

    let mut rng = {
        let datetime = settings.date_seed.unwrap_or(Utc::now());
        let datetime = floor_to_hour(datetime).expect("unable to floor to hour the datetime");

        println!("{:?}", datetime);

        let mut s = DefaultHasher::new();
        datetime.hash(&mut s);
        let hash = s.finish();

        StdRng::from_seed(&[hash as usize])
    };

    let dimensions = settings.screen_dimensions.unwrap_or_default().tuple();
    let (width, height) = dimensions;
    let mut camera = Camera::new([width as f64, height as f64]);

    let (fractal, mut zoom_divisions): (Box<Fractal>, _) = match rng.gen() {
        FractalType::Mandelbrot => {
            println!("Mandelbrot");

            let fractal = Mandelbrot::new();
            let zoom_divisions = rng.gen_range(3, 40);

            (Box::new(fractal), zoom_divisions)
        },
        FractalType::Julia => {
            // https://upload.wikimedia.org/wikipedia/commons/a/a9/Julia-Teppich.png
            let sub_gradients = Gradient::new(vec![
                SubGradient::new(ComplexPalette::new(-0.8,  0.4), ComplexPalette::new(-0.8,  0.0)),
                SubGradient::new(ComplexPalette::new(-0.6,  0.8), ComplexPalette::new(-0.6,  0.6)),
                SubGradient::new(ComplexPalette::new(-0.4,  0.8), ComplexPalette::new(-0.4,  0.6)),
                SubGradient::new(ComplexPalette::new(-0.2,  1.0), ComplexPalette::new(-0.2,  0.8)),
                SubGradient::new(ComplexPalette::new( 0.0,  1.0), ComplexPalette::new( 0.0,  0.8)),
                SubGradient::new(ComplexPalette::new( 0.19, 0.8), ComplexPalette::new( 0.19, 0.6)),
                SubGradient::new(ComplexPalette::new( 0.49, 0.6), ComplexPalette::new( 0.49, 0.2)),
            ]);

            let sub_gradient = sub_gradients.get(rng.gen());
            let gradient = sub_gradient.gradient();
            let ComplexPalette(Complex64 { re, im }) = gradient.get(rng.gen());

            println!("Julia ({}, {})", re, im);

            let fractal = Julia::new(re, im);
            let zoom_divisions = rng.gen_range(0, 40);

            (Box::new(fractal), zoom_divisions)
        },
    };

    println!("zoom divisions {:?}", zoom_divisions);

    // to zoom in the fractal:
    //   - find a good target point using the current camera
    //   - zoom using the camera into the current image
    //   - repeat the first step until the max number of iteration is reached
    //     or a target point can't be found
    while let Some((x, y)) = find_target_point(&mut rng, &fractal, &camera, dimensions) {
        let zoom = camera.zoom;
        camera.target_on([x as f64, y as f64], zoom * 0.5); // FIXME handle overflow

        if settings.produce_debug_images {
            produce_debug_image(&fractal, &camera, dimensions, zoom_divisions);
        }

        zoom_divisions -= 1;
        if zoom_divisions == 0 { break }
    }

    println!("camera: {:#?}", camera);

    let gradient = Gradient::with_domain(vec![
        (0.0,    LinSrgb::new(0.0,   0.027, 0.392)), // 0,    2.7,  39.2
        (0.16,   LinSrgb::new(0.125, 0.42,  0.796)), // 12.5, 42,   79.6
        (0.42,   LinSrgb::new(0.929, 1.0,   1.0)),   // 92.9, 100,  100
        (0.6425, LinSrgb::new(1.0,   0.667, 0.0)),   // 100,  66.7, 0
        (0.8575, LinSrgb::new(0.0,   0.008, 0.0)),   // 0,    0.8,  0
        (1.0,    LinSrgb::new(0.0,   0.0,   0.0)),   // 0,    0,    0
    ]);

    let painter = |i| {
        let color = gradient.get(i as f32 / 255.0);
        image::Rgb { data: color.into_pixel() }
    };

    let aa = settings.antialiazing as f64;
    let (bwidth, bheight) = (width * aa as u32, height * aa as u32);
    camera.screen_size = [bwidth as f64, bheight as f64];

    let image = produce_image(&fractal, &camera, (bwidth, bheight), painter);
    let image = imageops::resize(&image, width, height, FilterType::Triangle);

    image.save("./image.png").unwrap();
}
