#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include <windows.h>
#include "fork.h"

int main(int argc, const char* argv[])
{
	const int pid = fork();

	assert(pid > 0);

	switch (pid) {
	case 0: //child
	{
		printf("I am child.\n");
		break;
	}
	default: //parent
		printf("I am parent. Child process PID: %d\n", pid);
		break;
	}
	Sleep(1000);

	exit(0);
}