
public class FieldAccess {


	public FieldAccess() {
		myCount = globalCount - 2;
	}


	public static int globalCount = 0;
	public int myCount = 2;

	public static void main(String[] args) {
		globalCount += 4;
		FieldAccess ac = new FieldAccess();
		assert ac.myCount == 2;
	}
}