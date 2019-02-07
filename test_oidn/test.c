#include <stdio.h>
#include <oidn.h>
#define STB_IMAGE_IMPLEMENTATION
#define STB_IMAGE_WRITE_IMPLEMENTATION
#include "stb_image.h"
#include "stb_image_write.h"

int main(int argc, char **argv) {
	int x, y, n;
	unsigned char *data = stbi_load(argv[1], &x, &y, &n, 0);
	if (n != 3) {
		printf("Wrong number of image channels\n");
		return 1;
	}

	float *input_buf = (float*)malloc(sizeof(float) * x * y * n);
	float *output_buf = (float*)malloc(sizeof(float) * x * y * n);
	for (int i = 0; i < x * y * n; ++i) {
		input_buf[i] = data[i] / 255.0f;
	}

	OIDNDevice device = oidnNewDevice(OIDN_DEVICE_TYPE_DEFAULT);
	oidnCommitDevice(device);

	// Create a denoising filter
	OIDNFilter filter = oidnNewFilter(device, "RT");
	oidnSetSharedFilterImage(filter, "color",  input_buf,
			OIDN_FORMAT_FLOAT3, x, y, 0, 0, 0);
	oidnSetSharedFilterImage(filter, "output", output_buf,
			OIDN_FORMAT_FLOAT3, x, y, 0, 0, 0);
	oidnSetFilter1b(filter, "srgb", true);
	oidnCommitFilter(filter);

	// Filter the image
	oidnExecuteFilter(filter);

	// Check for errors
	const char* err;
	if (oidnGetDeviceError(device, &err) != OIDN_ERROR_NONE) {
		printf("Error: %s\n", err);
	}

	// Cleanup
	oidnReleaseFilter(filter);
	oidnReleaseDevice(device);

	for (int i = 0; i < x * y * n; ++i) {
		data[i] = output_buf[i] * 255.0f;
	}

	stbi_write_jpg(argv[2], x, y, n, data, 90);

	stbi_image_free(data);
	free(input_buf);
	free(output_buf);

	return 0;
}

