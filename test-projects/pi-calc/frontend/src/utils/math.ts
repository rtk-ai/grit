/**
 * Returns the first 1000 known digits of pi as a string (without the leading "3.").
 */
export function knownPiDigits(): string {
  return (
    "3." +
    "14159265358979323846264338327950288419716939937510" +
    "58209749445923078164062862089986280348253421170679" +
    "82148086513282306647093844609550582231725359408128" +
    "48111745028410270193852110555964462294895493038196" +
    "44288109756659334461284756482337867831652712019091" +
    "45648566923460348610454326648213393607260249141273" +
    "72458700660631558817488152092096282925409171536436" +
    "78925903600113305305488204665213841469519415116094" +
    "33057270365759591953092186117381932611793105118548" +
    "07446237996274956735188575272489122793818301194912" +
    "98336733624406566430860213949463952247371907021798" +
    "60943702770539217176293176752384674818467669405132" +
    "00056812714526356082778577134275778960917363717872" +
    "14684409012249534301465495853710507922796892589235" +
    "42019956112129021960864034418159813629774771309960" +
    "51870721134999999837297804995105973173281609631859" +
    "50244594553469083026425223082533446850352619311881" +
    "71010003137838752886587533208381420617177669147303" +
    "59825349042875546873115956286388235378759375195778" +
    "18577805321712268066130019278766111959092164201989"
  );
}

/**
 * Count how many correct digits a calculated value has when compared to known pi.
 * Compares character-by-character starting from the first digit.
 */
export function countCorrectDigits(calculated: number): number {
  const known = knownPiDigits();
  const calcStr = calculated.toFixed(15);

  let correct = 0;
  const knownClean = known.replace(".", "");
  const calcClean = calcStr.replace(".", "").replace("-", "");

  for (let i = 0; i < Math.min(knownClean.length, calcClean.length); i++) {
    if (knownClean[i] === calcClean[i]) {
      correct++;
    } else {
      break;
    }
  }

  return correct;
}

/**
 * Estimate the number of iterations needed for a given algorithm to achieve
 * a target number of correct digits.
 */
export function estimateIterations(algo: string, targetDigits: number): number {
  switch (algo) {
    case "leibniz":
      return Math.ceil(Math.pow(10, targetDigits));
    case "monte_carlo":
      return Math.ceil(Math.pow(10, targetDigits * 2));
    case "nilakantha":
      return Math.ceil(Math.pow(10, targetDigits / 3));
    case "chudnovsky":
      return Math.ceil(targetDigits / 14) + 1;
    case "wallis":
      return Math.ceil(Math.pow(10, targetDigits));
    case "ramanujan":
      return Math.ceil(targetDigits / 8) + 1;
    case "bbp":
      return Math.ceil(targetDigits / 1.2);
    case "gauss_legendre":
      return Math.ceil(Math.log2(targetDigits)) + 1;
    default:
      return Math.ceil(Math.pow(10, targetDigits));
  }
}

/**
 * Return the theoretical convergence rate description for an algorithm.
 * Returns digits gained per iteration (approximate).
 */
export function convergenceRate(algo: string): number {
  switch (algo) {
    case "leibniz":
      return 0.3;
    case "monte_carlo":
      return 0.15;
    case "nilakantha":
      return 1.0;
    case "chudnovsky":
      return 14.18;
    case "wallis":
      return 0.3;
    case "ramanujan":
      return 8.0;
    case "bbp":
      return 1.2;
    case "gauss_legendre":
      return -1;  // Doubles each iteration, not linear
    default:
      return 0;
  }
}
