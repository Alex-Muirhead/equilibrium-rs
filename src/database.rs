use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, digit1, line_ending, multispace0, space0, space1},
    combinator::{opt, recognize},
    multi::many0,
    number::complete::double,
    sequence::delimited,
};

#[derive(Debug, Clone)]
pub struct ThermoFile {
    pub header: ThermoHeader,
    pub species: Vec<Species>,
}

#[derive(Debug, Clone)]
pub struct ThermoHeader {
    pub temp_ranges: [f64; 4], // 200.00, 1000.00, 6000.00, 20000.0
    pub date: String,          // 9/09/04
}

#[derive(Debug, Clone)]
pub struct Species {
    pub name: String,
    pub description: String,
    pub elements: Vec<(String, f64)>, // Element name and count
    pub molecular_weight: f64,
    pub heat_of_formation: f64,
    pub temperature_ranges: Vec<TemperatureRange>,
}

#[derive(Debug, Clone)]
pub struct TemperatureRange {
    pub temp_low: f64,
    pub temp_high: f64,
    pub coefficients: [f64; 7],          // NASA polynomial coefficients
    pub integration_constants: [f64; 2], // Last two values on coefficient lines
}

// Parse scientific notation with 'D' instead of 'E' (common in Fortran)
fn parse_scientific_d(input: &str) -> IResult<&str, f64> {
    let (input, sign) = opt(alt((char('+'), char('-')))).parse(input)?;
    let (input, mantissa) = recognize((digit1, opt((char('.'), digit1)))).parse(input)?;
    let (input, _) = char('D')(input)?;
    let (input, exp_sign) = opt(alt((char('+'), char('-')))).parse(input)?;
    let (input, exponent) = digit1(input)?;

    // Convert to standard scientific notation and parse
    let sign_str = sign.map(|c| c.to_string()).unwrap_or_default();
    let exp_sign_str = exp_sign.map(|c| c.to_string()).unwrap_or_default();
    let scientific_str = format!("{}{}E{}{}", sign_str, mantissa, exp_sign_str, exponent);

    match scientific_str.parse::<f64>() {
        Ok(val) => Ok((input, val)),
        Err(_) => Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Float,
        ))),
    }
}

// Parse regular floating point number
fn parse_float(input: &str) -> IResult<&str, f64> {
    alt((parse_scientific_d, double)).parse(input)
}

// Parse whitespace-separated floating point number
fn parse_spaced_float(input: &str) -> IResult<&str, f64> {
    delimited(space0, parse_float, space0).parse(input)
}

// Parse the main header line
fn parse_header(input: &str) -> IResult<&str, ThermoHeader> {
    let (input, _) = tag("thermo")(input)?;
    let (input, _) = multispace0(input)?;
    let (input, temp1) = parse_spaced_float(input)?;
    let (input, temp2) = parse_spaced_float(input)?;
    let (input, temp3) = parse_spaced_float(input)?;
    let (input, temp4) = parse_spaced_float(input)?;
    let (input, _) = space0(input)?;
    let (input, date) = take_until("\n")(input)?;
    let (input, _) = line_ending(input)?;

    Ok((
        input,
        ThermoHeader {
            temp_ranges: [temp1, temp2, temp3, temp4],
            date: date.trim().to_string(),
        },
    ))
}

// Parse element composition (like "N   2.00O   2.00")
fn parse_elements(input: &str) -> IResult<&str, Vec<(String, f64)>> {
    let mut elements = Vec::new();
    let mut remaining = input;

    // Parse element-count pairs until we hit a number that looks like molecular weight
    while !remaining.is_empty() {
        let (rest, _) = space0(remaining)?;
        if rest.is_empty() {
            break;
        }

        // Try to parse an element name (letters)
        if let Ok((rest2, element)) =
            take_while1::<_, _, nom::error::Error<_>>(|c: char| c.is_alphabetic())(rest)
        {
            if element.len() > 0 {
                // Parse the count that follows
                if let Ok((rest3, count)) = parse_spaced_float(rest2) {
                    elements.push((element.to_string(), count));
                    remaining = rest3;
                    continue;
                }
            }
        }
        break;
    }

    Ok((remaining, elements))
}

// Parse species header line
fn parse_species_header(
    input: &str,
) -> IResult<&str, (String, String, Vec<(String, f64)>, f64, f64)> {
    let (input, name) = take_while1(|c: char| !c.is_whitespace())(input)?;
    let (input, _) = space1(input)?;

    // Parse description until we hit the element composition
    let (input, description_part) = take_until(" ")(input)?;
    let (input, _) = space1(input)?;

    // Parse remaining line to extract elements, molecular weight, and heat of formation
    let (input, rest_of_line) = take_until("\n")(input)?;
    let (input, _) = line_ending(input)?;

    // Parse elements from the rest of the line
    let (remaining, elements) = parse_elements(rest_of_line)?;

    // The remaining should have molecular weight and heat of formation
    let parts: Vec<&str> = remaining.trim().split_whitespace().collect();
    let molecular_weight = if parts.len() >= 2 {
        parts[parts.len() - 2].parse().unwrap_or(0.0)
    } else {
        0.0
    };
    let heat_of_formation = if parts.len() >= 1 {
        parts[parts.len() - 1].parse().unwrap_or(0.0)
    } else {
        0.0
    };

    Ok((
        input,
        (
            name.to_string(),
            description_part.to_string(),
            elements,
            molecular_weight,
            heat_of_formation,
        ),
    ))
}

// Parse temperature range with coefficients
fn parse_temperature_range(input: &str) -> IResult<&str, TemperatureRange> {
    // First line: temperature range and metadata
    let (input, _) = space0(input)?;
    let (input, temp_low) = parse_spaced_float(input)?;
    let (input, temp_high) = parse_spaced_float(input)?;
    let (input, _) = take_until("\n")(input)?; // Skip the rest of the metadata
    let (input, _) = line_ending(input)?;

    // Parse coefficient lines (typically 2 lines with scientific notation)
    let (input, coeff_line1) = take_until("\n")(input)?;
    let (input, _) = line_ending(input)?;
    let (input, coeff_line2) = take_until("\n")(input)?;
    let (input, _) = line_ending(input)?;

    // Parse coefficients from both lines
    let mut coefficients = [0.0; 7];
    let mut integration_constants = [0.0; 2];

    // Parse first coefficient line (usually has 5 coefficients)
    let coeff1_parts: Vec<&str> = coeff_line1.trim().split_whitespace().collect();
    for (i, part) in coeff1_parts.iter().take(5).enumerate() {
        if let Ok((_, val)) = parse_scientific_d(part) {
            coefficients[i] = val;
        }
    }

    // Parse second coefficient line (usually has 2 coefficients + 2 integration constants)
    let coeff2_parts: Vec<&str> = coeff_line2.trim().split_whitespace().collect();
    for (i, part) in coeff2_parts.iter().take(2).enumerate() {
        if let Ok((_, val)) = parse_scientific_d(part) {
            coefficients[5 + i] = val;
        }
    }

    // Get integration constants (last 2 values)
    if coeff2_parts.len() >= 4 {
        for (i, part) in coeff2_parts.iter().skip(coeff2_parts.len() - 2).enumerate() {
            if let Ok((_, val)) = parse_scientific_d(part) {
                integration_constants[i] = val;
            }
        }
    }

    Ok((
        input,
        TemperatureRange {
            temp_low,
            temp_high,
            coefficients,
            integration_constants,
        },
    ))
}

// Parse a complete species entry
fn parse_species(input: &str) -> IResult<&str, Species> {
    let (input, (name, description, elements, molecular_weight, heat_of_formation)) =
        parse_species_header(input)?;

    // Parse temperature ranges (usually 3 ranges)
    let mut temperature_ranges = Vec::new();
    let mut remaining = input;

    // Keep parsing temperature ranges until we hit "END REACTANTS" or another species
    while !remaining.trim_start().starts_with("END")
        && !remaining
            .trim_start()
            .chars()
            .next()
            .map(|c| c.is_alphabetic())
            .unwrap_or(false)
        && !remaining.trim().is_empty()
    {
        if let Ok((rest, temp_range)) = parse_temperature_range(remaining) {
            temperature_ranges.push(temp_range);
            remaining = rest;
        } else {
            break;
        }
    }

    Ok((
        remaining,
        Species {
            name,
            description,
            elements,
            molecular_weight,
            heat_of_formation,
            temperature_ranges,
        },
    ))
}

// Parse the complete thermo file
pub fn parse_thermo_file(input: &str) -> IResult<&str, ThermoFile> {
    let (input, _) = multispace0(input)?; // Skip any leading whitespace/comments
    let (input, header) = parse_header(input)?;
    let (input, species) = many0(parse_species).parse(input)?;
    let (input, _) = multispace0(input)?; // Skip trailing content

    Ok((input, ThermoFile { header, species }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scientific_d_parsing() {
        assert_eq!(parse_scientific_d("2.500000000D+00"), Ok(("", 2.5)));
        assert_eq!(parse_scientific_d("-7.453750000D+02"), Ok(("", -745.375)));
        assert_eq!(
            parse_scientific_d("1.066859930D-05"),
            Ok(("", 1.066859930e-5))
        );
    }

    #[test]
    fn test_header_parsing() {
        let input = "thermo                                                                          \n    200.00   1000.00   6000.00  20000.     9/09/04\n";
        let result = parse_header(input);
        assert!(result.is_ok());
    }
}
