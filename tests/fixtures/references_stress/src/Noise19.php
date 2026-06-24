<?php

namespace App;

class Noise19
{
    private function compute(): int
    {
        return 19;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
