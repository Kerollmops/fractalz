use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::BufWriter;

use egg_mode::{
    media::{media_types, UploadBuilder},
    tweet::DraftTweet,
    KeyPair, Token,
};
use frustalz::{
    generate::{DateSeed, Generator},
    image::{Antialiazing, ScreenDimensions},
};
use image::RgbImage;
use png::{Encoder, HasParameters};
use rand::{SeedableRng, StdRng};
use structopt::StructOpt;
use tokio_core::reactor;

#[derive(Debug, Clone, StructOpt)]
pub struct Settings {
    /// The date to use as a seed rounded to the hour
    #[structopt(long = "date-seed")]
    pub date_seed: Option<DateSeed>,

    /// Antialiazing used for the images generated (a power of 4)
    #[structopt(long = "antialiazing")]
    pub antialiazing: Option<Antialiazing>,

    /// Generate the image without uploading it
    #[structopt(long = "dry-run")]
    pub dry_run: bool,

    /// Generate the image and save it in a file
    #[structopt(long = "save-image")]
    pub save_image: bool,

    /// Dimensions of images used to dive into fractals
    #[structopt(long = "dive-dimensions")]
    pub dive_dimensions: Option<ScreenDimensions>,

    /// Dimensions of the final image generated
    #[structopt(long = "shot-dimensions")]
    pub shot_dimensions: Option<ScreenDimensions>,

    /// Whether the program produce all images while diving in the fractal
    #[structopt(long = "no-debug-images")]
    pub no_debug_images: bool,
}

fn image_to_png(image: RgbImage) -> Vec<u8> {
    let (width, height) = image.dimensions();
    let buf = image.into_raw();

    let mut out = BufWriter::new(Vec::new());

    {
        let mut encoder = Encoder::new(&mut out, width, height);
        encoder.set(png::ColorType::RGB).set(png::BitDepth::Eight);

        let mut writer = encoder.write_header().unwrap();

        writer.write_image_data(&buf).unwrap();
    }

    out.into_inner().unwrap()
}

fn main() {
    let settings = Settings::from_args();
    let mut core = reactor::Core::new().unwrap();

    let consumer_key = include_str!("consumer_key").trim();
    let consumer_secret = include_str!("consumer_secret").trim();
    let access_key = include_str!("access_key").trim();
    let access_secret = include_str!("access_secret").trim();

    let token = Token::Access {
        consumer: KeyPair::new(consumer_key, consumer_secret),
        access: KeyPair::new(access_key, access_secret),
    };
    let handle = core.handle();

    if let Err(err) = core.run(egg_mode::verify_tokens(&token, &handle)) {
        eprintln!("{}", err);
    } else {
        let rng = {
            let datetime = settings.date_seed.unwrap_or_default();
            println!("{:?}", datetime);

            let mut s = DefaultHasher::new();
            datetime.hash(&mut s);

            let hash = s.finish();
            StdRng::from_seed(&[hash as usize])
        };

        let mut generator = Generator::new(rng);

        if let Some(dims) = settings.shot_dimensions {
            generator.shot_dimensions(dims);
        }
        if let Some(dims) = settings.dive_dimensions {
            generator.dive_dimensions(dims);
        }
        if let Some(anti) = settings.antialiazing {
            generator.antialiazing(anti);
        }
        generator.debug_images(!settings.no_debug_images);

        let (info, image) = generator.generate();

        if settings.save_image || settings.dry_run {
            match image.save("./image.png") {
                Ok(_) => println!("image saved to \"./image.png\""),
                Err(e) => eprintln!("can not save image to \"./image.png\": {}", e),
            }
        }

        if !settings.dry_run {
            let image = image_to_png(image);
            let builder = UploadBuilder::new(image, media_types::image_png());
            let media_handle = core.run(builder.call(&token, &handle)).unwrap();

            let message = info.to_string();
            let draft = DraftTweet::new(message).media_ids(&[media_handle.id]);
            let tweet = core.run(draft.send(&token, &handle)).unwrap();

            if let Some(url) = tweet.entities.urls.first().map(|u| &u.url) {
                println!("tweet url: {}", url);
            }
        }
    }
}
