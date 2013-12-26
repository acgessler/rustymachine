// OUTCOME=PASS
// RETVAL=4

public class IntegerArithmetics {


	public static void main(String[] args) {

		int i = 0;
		assert i == 0;

		// basic calculations
		i += 2;
		assert i == 2;

		i *= i;
		assert i == 4;

		int a = -2;
		a = a + i + (-2);
		assert a == 0;

		// negation
		a = -a;
		assert a == 0;

		// assignment
		a = i;
		assert a == 4;

		// exact division
		a /= i;
		assert a == 1;

		// floor division
		a /= i;
		assert a == 0;

		// TODO: overflow, underflow
		System.exit(i);
	};
}