#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Copy)]
pub struct QoiInputBuffer<const N: usize> {
    pub data: [u8; N],
    pub capacity: usize,
    pub index: usize,
}

impl<const N: usize> QoiInputBuffer<N> {
    // sets index to start, capacity to input.len() and replaces input.len() bytes
    #[inline]
    pub const fn write_bytes(mut self, input: &[u8]) -> Self { // MUST: input.len() <= N
        self.capacity = input.len();
        self.index = 0;
        let mut index = 0;
        while index < input.len() {
            self.data[index] = input[index];
            index += 1;
        }
        self
    }
    #[inline]
    pub const fn read_byte(mut self) -> (Self, u8) { // MUST: capacity != index
        let output = self.data[self.index];
        self.index += 1;
        (self, output)
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Copy)]
pub struct QoiOutputBuffer<const N: usize> {
    pub data: [u8; N],
    pub space: usize,
}

impl<const N: usize> QoiOutputBuffer<N> {
    #[inline]
    pub const fn append_byte(mut self, input: u8) -> Self { // MUST: self.space != 0
        self.data[N - self.space] = input;
        self.space -= 1;
        self
    }
    #[inline]
    pub const fn append_bytes(mut self, input: &[u8]) -> Self { // MUST: input.len() <= self.space
        let mut input_index = 0;
        let mut index = N - self.space;
        while input_index < input.len() {
            self.data[index] = input[input_index];
            input_index += 1;
            index += 1;
            self.space -= 1;
        }
        self
    }
    #[inline]
    pub const fn get_all_bytes(mut self) -> (Self, [u8; N]) {
        self.space = N;
        (self, self.data)
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::is_identical;
    use super::{QoiInputBuffer, QoiOutputBuffer};
    #[test]
    const fn infallible_input_read_byte() {
        let mut buffer = QoiInputBuffer {data: [255, 56, 89, 0, 0], capacity: 3, index: 0};
        let byte; (buffer, byte) = buffer.read_byte();
        assert!(is_identical(&buffer.data, &[255, 56, 89, 0, 0]));
        assert!(buffer.capacity == 3);
        assert!(buffer.index == 1);
        assert!(byte == 255);
    }
    #[test]
    const fn infallible_input_write_bytes() {
        let input = [55, 33, 74];
        let mut buffer = QoiInputBuffer {data: [0, 0, 0, 0, 0], capacity: 0, index: 0};
        buffer = buffer.write_bytes(&input);
        assert!(is_identical(&buffer.data, &[55, 33, 74, 0, 0]));
        assert!(buffer.capacity == 3);
        assert!(buffer.index == 0);
    }
    #[test]
    const fn infallible_output_append_byte() {
        let mut buffer = QoiOutputBuffer {data: [255, 56, 89, 0, 0], space: 2};
        buffer = buffer.append_byte(79);
        assert!(is_identical(&buffer.data, &[255, 56, 89, 79, 0]));
        assert!(buffer.space == 1);
    }
    #[test]
    const fn infallible_output_append_bytes() {
        let mut buffer = QoiOutputBuffer {data: [255, 56, 0, 0, 0], space: 3};
        let input = [1, 77, 4];
        buffer = buffer.append_bytes(&input);
        assert!(is_identical(&buffer.data, &[255, 56, 1, 77, 4]));
        assert!(buffer.space == 0);
    }
    #[test]
    const fn infallible_output_get_all_bytes() {
        let mut buffer = QoiOutputBuffer {data: [255, 56, 0, 0, 0], space: 3};
        let output; (buffer, output) = buffer.get_all_bytes();
        assert!(is_identical(&output, &buffer.data));
        assert!(buffer.space == buffer.data.len());
    }
}
