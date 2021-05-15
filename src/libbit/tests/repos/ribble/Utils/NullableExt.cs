using System;

namespace RibbleChatServer.Utils
{
    public static class NullableExt
    {
        /// optional fmap
        public static U? Map<T, U>(this T? x, Func<T, U> f)
        {
            if (x is null) return default(U);
            return f(x);
        }

    }
}