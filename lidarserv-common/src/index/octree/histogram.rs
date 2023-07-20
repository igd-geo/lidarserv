use std::fmt::Debug;
use std::ops::{Add, AddAssign};
use std::ops::Sub;
use std::ops::Div;
use log::debug;
use num_traits::{FromPrimitive, Num, One, ToPrimitive};
use serde::{Serialize, Deserialize};

// Define a generic histogram struct

/// A histogram of values of type T
/// Multiple implementations are provided for different types (u8, u16, i16)
/// for performance and memory reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Histogram<T> {
    bins: Vec<u64>,
    min_value: T,
    max_value: T,
    bin_width: T,
}

impl Histogram<u8> {
    pub fn new(min_value: u8, max_value: u8, num_bins: usize) -> Self {
        assert!(min_value < max_value, "min_value must be less than max_value");
        assert!(num_bins > 0, "num_bins must be greater than 0");
        assert!(num_bins <= (max_value as u16 - min_value as u16) as usize + 1, "num_bins must be less than or equal than the number of possible values");

        // calculate the bin width
        // has to be casted to u16 to avoid overflow (e.g. 255 - 0 + 1 = 256)
        let bin_width = (((max_value as u16 - min_value as u16) + 1) / num_bins as u16) as u8;
        assert!(bin_width > 0, "bin_width must be greater than 0");

        let bins = vec![0; num_bins];

        Histogram {
            bins,
            min_value,
            max_value,
            bin_width,
        }
    }

    // Increment the appropriate bin for the given value
    pub fn add(&mut self, mut value: u8) {
        if value < self.min_value {debug!("value {} is smaller than min_value {}", value, self.min_value); value = self.min_value;}
        if value > self.max_value {debug!("value {} is larger than max_value {}", value, self.max_value); value = self.max_value;}

        let bin_index = self.get_bin_index(value);
        self.bins[bin_index] += 1;
    }

    // calculate bin index from value
    pub fn get_bin_index(&self, mut value: u8) -> usize {
        if value < self.min_value {debug!("value {} is smaller than min_value {}", value, self.min_value); value = self.min_value;}
        if value > self.max_value {debug!("value {} is larger than max_value {}", value, self.max_value); value = self.max_value;}

        let bin_index = ((value as u16 - self.min_value as u16) / self.bin_width as u16).to_usize().unwrap().min(self.bins.len() - 1);
        bin_index
    }

    // Check if a given range contains values in the histogram
    pub fn range_contains_values(&self, range: (u8,u8)) -> bool {
        assert!(range.0 <= self.max_value && range.1 >= self.min_value);
        let min_bin_index = self.get_bin_index(range.0);
        let max_bin_index = self.get_bin_index(range.1);
        for i in min_bin_index..max_bin_index+1 {
            if self.bins[i] > 0 {
                return true;
            }
        }
        false
    }
}

impl Histogram<u16> {
    pub fn new(min_value: u16, max_value: u16, num_bins: usize) -> Self {
        assert!(min_value < max_value, "min_value must be less than max_value");
        assert!(num_bins > 0, "num_bins must be greater than 0");
        assert!(num_bins <= (max_value as u32 - min_value as u32) as usize + 1, "num_bins must be less than or equal than the number of possible values");

        let bin_width = (((max_value as u32 - min_value as u32) + 1) / num_bins as u32) as u16;
        assert!(bin_width > 0, "bin_width must be greater than 0");

        let bins = vec![0; num_bins];

        Histogram {
            bins,
            min_value,
            max_value,
            bin_width,
        }
    }

    // Increment the appropriate bin for the given value
    pub fn add(&mut self, mut value: u16) {
        if value < self.min_value {debug!("value {} is smaller than min_value {}", value, self.min_value); value = self.min_value;}
        if value > self.max_value {debug!("value {} is larger than max_value {}", value, self.max_value); value = self.max_value;}

        let bin_index = self.get_bin_index(value);
        self.bins[bin_index] += 1;
    }

    // calculate bin index from value
    pub fn get_bin_index(&self, mut value: u16) -> usize {
        if value < self.min_value {debug!("value {} is smaller than min_value {}", value, self.min_value); value = self.min_value;}
        if value > self.max_value {debug!("value {} is larger than max_value {}", value, self.max_value); value = self.max_value;}

        let bin_index = ((value as u32 - self.min_value as u32) / self.bin_width as u32).to_usize().unwrap().min(self.bins.len() - 1);
        bin_index
    }

    // Check if a given range contains values in the histogram
    pub fn range_contains_values(&self, range: (u16,u16)) -> bool {
        assert!(range.0 <= self.max_value && range.1 >= self.min_value);
        let min_bin_index = self.get_bin_index(range.0);
        let max_bin_index = self.get_bin_index(range.1);
        for i in min_bin_index..max_bin_index+1 {
            if self.bins[i] > 0 {
                return true;
            }
        }
        false
    }
}

impl Histogram<i8> {
    pub fn new(min_value: i8, max_value: i8, num_bins: usize) -> Self {
        assert!(min_value < max_value, "min_value must be less than max_value");
        assert!(num_bins > 0, "num_bins must be greater than 0");
        assert!(num_bins <= (max_value as i16 - min_value as i16) as usize + 1, "num_bins must be less than or equal than the number of possible values");

        let bin_width = (((max_value as i16 - min_value as i16) + 1) / num_bins as i16) as i8;
        assert!(bin_width > 0, "bin_width must be greater than 0");

        let bins = vec![0; num_bins];

        Histogram {
            bins,
            min_value,
            max_value,
            bin_width,
        }
    }

    // Increment the appropriate bin for the given value
    pub fn add(&mut self, mut value: i8) {
        if value < self.min_value {debug!("value {} is smaller than min_value {}", value, self.min_value); value = self.min_value;}
        if value > self.max_value {debug!("value {} is larger than max_value {}", value, self.max_value); value = self.max_value;}

        let bin_index = self.get_bin_index(value);
        self.bins[bin_index] += 1;
    }

    // calculate bin index from value
    pub fn get_bin_index(&self, mut value: i8) -> usize {
        if value < self.min_value {debug!("value {} is smaller than min_value {}", value, self.min_value); value = self.min_value;}
        if value > self.max_value {debug!("value {} is larger than max_value {}", value, self.max_value); value = self.max_value;}

        let bin_index = ((value as i16 - self.min_value as i16) / self.bin_width as i16).to_usize().unwrap().min(self.bins.len() - 1);
        bin_index
    }

    // Check if a given range contains values in the histogram
    pub fn range_contains_values(&self, range: (i8,i8)) -> bool {
        assert!(range.0 <= self.max_value && range.1 >= self.min_value);
        let min_bin_index = self.get_bin_index(range.0);
        let max_bin_index = self.get_bin_index(range.1);
        for i in min_bin_index..max_bin_index+1 {
            if self.bins[i] > 0 {
                return true;
            }
        }
        false
    }
}

impl<T: Num + Copy + PartialOrd + ToPrimitive + FromPrimitive> Histogram<T> {

    // Get the number of values in the specified bin
    pub fn get_bin_count(&self, bin_index: usize) -> Option<u64> {
        self.bins.get(bin_index).copied()
    }

    // Get the number of bins in the histogram
    pub fn num_bins(&self) -> usize {
        self.bins.len()
    }

    // Get the minimum value of the histogram range
    pub fn min_value(&self) -> T {
        self.min_value
    }

    // Get the maximum value of the histogram range
    pub fn max_value(&self) -> T {
        self.max_value
    }

    // Add two histograms together
    pub fn add_histogram(&mut self, other: &Histogram<T>) {
        assert!(self.min_value == other.min_value, "Histograms must have the same minimum value");
        assert!(self.max_value == other.max_value, "Histograms must have the same maximum value");
        assert_eq!(self.num_bins(), other.num_bins(), "Histograms must have the same number of bins");

        for i in 0..self.num_bins() {
            self.bins[i] += other.bins[i];
        }
    }
}


#[cfg(test)]
mod tests {
    use std::ops::Add;
    use super::*;

    #[test]
    fn general_test() {
        let min = 0;
        let max = 10;
        let num_bins = 5;
        let mut histogram = Histogram::<u8>::new(min, max, num_bins);

        assert_eq!(histogram.min_value(), min);
        assert_eq!(histogram.max_value(), max);
        assert_eq!(histogram.num_bins(), num_bins);

        for i in min .. max {
            histogram.add(i);
        }

        for i in 0 .. num_bins {
            assert_eq!(histogram.get_bin_count(i), Some(2));
        }

        assert!(histogram.range_contains_values((0, 10)));
        assert!(histogram.range_contains_values((0, 5)));
        assert!(histogram.range_contains_values((5, 10)));
        assert!(histogram.range_contains_values((2, 8)));
        assert!(histogram.range_contains_values((2, 2)));
    }

    #[test]
    fn test_range() {
        let min = 0;
        let max = 10;
        let num_bins = 5;
        let mut histogram = Histogram::<u8>::new(min, max, num_bins);

        histogram.add(1);
        histogram.add(2);
        histogram.add(3);
        histogram.add(8);
        histogram.add(9);
        histogram.add(10);

        assert!(histogram.range_contains_values((0, 10)));
        assert!(histogram.range_contains_values((0, 5)));
        assert!(histogram.range_contains_values((3, 4)));
        assert!(histogram.range_contains_values((7, 8)));
        assert!(!histogram.range_contains_values((4, 7)));
        assert!(!histogram.range_contains_values((4, 4)));
        assert!(!histogram.range_contains_values((7, 7)));
    }

    #[test]
    fn test_negative() {
        let min = -90;
        let max = 90;
        let num_bins = 25;
        let mut histogram = Histogram::<i8>::new(min, max, num_bins);

        histogram.add(-90);
        histogram.add(-45);
        histogram.add(0);
        histogram.add(45);
        histogram.add(90);

        assert!(histogram.range_contains_values((-90, 90)));

        for i in 0 .. num_bins {
            println!("{:?}", histogram.get_bin_count(i));
        }
    }

    #[test]
    fn add_histograms() {
        let min = 0;
        let max = 10;
        let num_bins = 5;
        let mut histogram1 = Histogram::<u8>::new(min, max, num_bins);
        let mut histogram2 = Histogram::<u8>::new(min, max, num_bins);

        for i in min .. max {
            histogram1.add(i);
            histogram2.add(i);
        }

        histogram1.add_histogram(&histogram2);

        for i in 0 .. num_bins {
            assert_eq!(histogram1.get_bin_count(i), Some(4));
        }
    }

    #[test]
    #[should_panic]
    fn test_incompatible_histograms() {
        let min = 0;
        let max = 10;
        let num_bins = 5;
        let mut histogram1 = Histogram::<u8>::new(min, max, num_bins);
        let mut histogram2 = Histogram::<u8>::new(min, max, num_bins + 1);

        histogram1.add_histogram(&histogram2);
    }
}