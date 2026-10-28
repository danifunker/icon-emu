//! Intel 8255 PPI, mode 0 only (the ICON ROM never uses modes 1/2).

#[derive(Clone, Debug)]
pub struct Ppi8255 {
    /// Output latches for ports A, B, C.
    pub latch: [u8; 3],
    /// Control word (last mode-set).
    pub control: u8,
}

impl Default for Ppi8255 {
    fn default() -> Self {
        Self { latch: [0; 3], control: 0x9B } // reset: all ports input
    }
}

impl Ppi8255 {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn a_is_input(&self) -> bool {
        self.control & 0x10 != 0
    }
    fn b_is_input(&self) -> bool {
        self.control & 0x02 != 0
    }
    fn c_upper_is_input(&self) -> bool {
        self.control & 0x08 != 0
    }
    fn c_lower_is_input(&self) -> bool {
        self.control & 0x01 != 0
    }

    /// Read a port (0=A, 1=B, 2=C, 3=control). `inputs` are the external
    /// pin levels for A, B, C; output bits read back their latch.
    pub fn read(&self, reg: u8, inputs: [u8; 3]) -> u8 {
        match reg & 3 {
            0 => if self.a_is_input() { inputs[0] } else { self.latch[0] },
            1 => if self.b_is_input() { inputs[1] } else { self.latch[1] },
            2 => {
                let hi = if self.c_upper_is_input() { inputs[2] } else { self.latch[2] } & 0xF0;
                let lo = if self.c_lower_is_input() { inputs[2] } else { self.latch[2] } & 0x0F;
                hi | lo
            }
            _ => 0xFF, // control register is write-only on a real 8255
        }
    }

    pub fn write(&mut self, reg: u8, val: u8) {
        match reg & 3 {
            0 => self.latch[0] = val,
            1 => self.latch[1] = val,
            2 => self.latch[2] = val,
            _ => {
                if val & 0x80 != 0 {
                    // Mode set clears all output latches.
                    self.control = val;
                    self.latch = [0; 3];
                } else {
                    // Port C bit set/reset.
                    let bit = (val >> 1) & 7;
                    if val & 1 != 0 {
                        self.latch[2] |= 1 << bit;
                    } else {
                        self.latch[2] &= !(1 << bit);
                    }
                }
            }
        }
    }

    /// Current level driven on port C by the output latch (only meaningful
    /// for bits configured as outputs).
    pub fn port_c_out(&self) -> u8 {
        self.latch[2]
    }
    pub fn port_b_out(&self) -> u8 {
        self.latch[1]
    }
    pub fn port_a_out(&self) -> u8 {
        self.latch[0]
    }
}
